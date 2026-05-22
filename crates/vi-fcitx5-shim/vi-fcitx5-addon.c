/*
 * vi-fcitx5-addon.c — Minimal Fcitx5 addon shim for vi-daemon.
 *
 * Despite the .c extension this file is C++17.  Compile with:
 *
 *   g++ -std=c++17 -shared -fPIC -o libvi-fcitx5.so vi-fcitx5-addon.c \
 *       $(pkg-config --cflags --libs Fcitx5Core)
 *
 * Install the shared library where Fcitx5 searches for addons; the exact
 * path is distro-dependent (e.g. /usr/lib/x86_64-linux-gnu/fcitx5/ or
 * $XDG_DATA_HOME/fcitx5/).  The .conf files are written by vi-daemon at
 * startup — restart Fcitx5 after first run so it picks them up.
 *
 * # Protocol
 * The shim connects to vi-daemon's Unix socket (see engine_socket_path() in
 * lib.rs) and exchanges newline-terminated text lines:
 *
 *   Shim → daemon:
 *     ACTIVATE <ctx_id>\n
 *     DEACTIVATE <ctx_id>\n
 *     KEY <keyval> <keycode> <state> <serial>\n
 *     KEYUP <keyval>\n
 *
 *   Daemon → shim (one response per request):
 *     RESULT <consumed:0|1> <num_ops>\n
 *     OP COMMIT <hex_utf8_text>\n
 *     OP PREEDIT <cursor_char_count> <hex_utf8_text>\n
 *     OP CLEAR\n
 *     OP FWDKEY <keyval> <keycode> <state>\n
 *
 *   hex_utf8_text: every UTF-8 byte as two lowercase hex digits, no separator.
 */

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/key.h>
#include <fcitx/event.h>

#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

// ── Socket path ───────────────────────────────────────────────────────────────

static std::string socket_path() {
    const char *xdg = getenv("XDG_DATA_HOME");
    std::string base;
    if (xdg && *xdg) {
        base = xdg;
    } else {
        const char *home = getenv("HOME");
        base = home ? std::string(home) + "/.local/share" : "/tmp";
    }
    return base + "/vi-ime/engine.sock";
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/* Read bytes until '\n', storing them (without the newline) in buf.
 * Returns number of bytes stored, or -1 on EOF/error. */
static ssize_t read_line(int fd, char *buf, size_t max) {
    size_t n = 0;
    while (n + 1 < max) {
        char c;
        ssize_t r;
        do { r = ::read(fd, &c, 1); } while (r < 0 && errno == EINTR);
        if (r <= 0) return -1;
        if (c == '\n') break;
        buf[n++] = c;
    }
    buf[n] = '\0';
    return static_cast<ssize_t>(n);
}

/* Write all bytes in buf to fd, retrying on EINTR. */
static bool write_all(int fd, const char *buf, size_t len) {
    while (len > 0) {
        ssize_t r;
        do { r = ::write(fd, buf, len); } while (r < 0 && errno == EINTR);
        if (r <= 0) return false;
        buf += r;
        len -= static_cast<size_t>(r);
    }
    return true;
}

/* Decode hex string (pairs of hex digits) into out[].
 * Writes a NUL terminator; returns number of decoded bytes (excluding NUL). */
static size_t hex_decode(const char *hex, char *out, size_t max) {
    size_t n = 0;
    while (hex[0] && hex[1] && n + 1 < max) {
        char h[3] = {hex[0], hex[1], '\0'};
        out[n++] = static_cast<char>(strtol(h, nullptr, 16));
        hex += 2;
    }
    out[n] = '\0';
    return n;
}

// ── ViImeEngine ───────────────────────────────────────────────────────────────

class ViImeEngine : public fcitx::InputMethodEngine {
    int sockfd_ = -1;

    /* Connect to vi-daemon.  Returns true on success. */
    bool connect_daemon() {
        if (sockfd_ >= 0) return true;

        int fd = ::socket(AF_UNIX, SOCK_STREAM, 0);
        if (fd < 0) return false;

        /* 100 ms receive/send timeout prevents blocking Fcitx5's event loop. */
        struct timeval tv{0, 100000};
        ::setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
        ::setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv));

        struct sockaddr_un addr{};
        addr.sun_family = AF_UNIX;
        std::string path = socket_path();
        ::strncpy(addr.sun_path, path.c_str(), sizeof(addr.sun_path) - 1);

        if (::connect(fd, reinterpret_cast<struct sockaddr *>(&addr),
                      sizeof(addr)) < 0) {
            ::close(fd);
            return false;
        }
        sockfd_ = fd;
        return true;
    }

    void disconnect_daemon() {
        if (sockfd_ >= 0) {
            ::close(sockfd_);
            sockfd_ = -1;
        }
    }

    /*
     * Execute one OP line on the given InputContext.
     */
    static void execute_op(const char *line, fcitx::InputContext *ic) {
        if (!ic) return;

        if (strncmp(line, "OP COMMIT ", 10) == 0) {
            char text[65536];
            hex_decode(line + 10, text, sizeof(text));
            ic->commitString(text);

        } else if (strncmp(line, "OP PREEDIT ", 11) == 0) {
            /* Parse "OP PREEDIT <cursor_chars> <hex_text>" */
            const char *p = line + 11;
            char *after_cursor = nullptr;
            long cursor_chars = strtol(p, &after_cursor, 10);
            const char *hex = (after_cursor && *after_cursor == ' ')
                                ? after_cursor + 1
                                : nullptr;

            char text[65536];
            size_t text_len = hex ? hex_decode(hex, text, sizeof(text)) : 0u;

            /* Convert char-count cursor to UTF-8 byte offset. */
            int cursor_bytes = 0;
            {
                long rem = cursor_chars;
                const unsigned char *t =
                    reinterpret_cast<const unsigned char *>(text);
                while (rem > 0 && *t) {
                    unsigned char c = *t;
                    int clen = (c < 0x80) ? 1
                             : (c < 0xE0) ? 2
                             : (c < 0xF0) ? 3
                             :               4;
                    cursor_bytes += clen;
                    t += clen;
                    --rem;
                }
            }

            fprintf(stderr, "[vi-fcitx5] preedit update: cursor=%ld byte_offset=%d text=\"%.*s\"\n",
                    cursor_chars, cursor_bytes, (int)text_len, text);

            fcitx::Text preedit;
            preedit.append(std::string(text, text_len),
                           fcitx::TextFormatFlag::Underline);
            preedit.setCursor(cursor_bytes);
            ic->inputPanel().setClientPreedit(preedit);
            ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);

        } else if (strcmp(line, "OP CLEAR") == 0) {
            fprintf(stderr, "[vi-fcitx5] preedit cleared\n");
            ic->inputPanel().setClientPreedit(fcitx::Text{});
            ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);

        } else if (strncmp(line, "OP FWDKEY ", 10) == 0) {
            unsigned kv = 0, kc = 0, st = 0;
            sscanf(line + 10, "%u %u %u", &kv, &kc, &st);
            ic->forwardKey(
                fcitx::Key(static_cast<fcitx::KeySym>(kv),
                           fcitx::KeyStates(st),
                           kc));
        }
    }

    /*
     * Send `req` to vi-daemon, read the RESULT line and all OP lines.
     * Executes each OP on `ic` (may be null for ops that don't need it).
     * Returns false if the daemon is unreachable; sets *consumed_out if nonnull.
     */
    bool transact(const char *req, bool *consumed_out,
                  fcitx::InputContext *ic) {
        if (!connect_daemon()) return false;

        size_t req_len = strlen(req);
        if (!write_all(sockfd_, req, req_len)) {
            disconnect_daemon();
            return false;
        }

        char line[65536];
        if (read_line(sockfd_, line, sizeof(line)) < 0) {
            disconnect_daemon();
            return false;
        }

        int consumed = 0, num_ops = 0;
        if (sscanf(line, "RESULT %d %d", &consumed, &num_ops) != 2) {
            disconnect_daemon();
            return false;
        }
        if (consumed_out) *consumed_out = (consumed != 0);

        for (int i = 0; i < num_ops; ++i) {
            if (read_line(sockfd_, line, sizeof(line)) < 0) {
                disconnect_daemon();
                return false;
            }
            fprintf(stderr, "[vi-fcitx5] recv op[%d/%d]: %s\n", i + 1, num_ops, line);
            execute_op(line, ic);
        }
        return true;
    }

public:
    explicit ViImeEngine(fcitx::AddonManager * /*manager*/) {}
    ~ViImeEngine() override { disconnect_daemon(); }

    /* Called when a key is pressed or released in the active input context. */
    void keyEvent(const fcitx::InputMethodEntry & /*entry*/,
                  fcitx::KeyEvent &event) override {
        auto *ic = event.inputContext();

        if (event.isRelease()) {
            /* Inform vi-daemon so it can reset its repeat-guard. */
            char req[128];
            snprintf(req, sizeof(req), "KEYUP %u\n",
                     static_cast<unsigned>(event.key().sym()));
            bool dummy = false;
            transact(req, &dummy, nullptr);
            return;
        }

        char req[256];
        snprintf(req, sizeof(req), "KEY %u %u %u 0\n",
                 static_cast<unsigned>(event.key().sym()),
                 static_cast<unsigned>(event.key().code()),
                 static_cast<unsigned>(event.key().states()));

        bool consumed = false;
        if (!transact(req, &consumed, ic)) return; /* daemon unreachable */
        if (consumed) event.filterAndAccept();
    }

    /* Called when this input method is activated for a new input context. */
    void activate(const fcitx::InputMethodEntry & /*entry*/,
                  fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        char req[64];
        snprintf(req, sizeof(req), "ACTIVATE %p\n",
                 static_cast<void *>(ic));
        bool dummy = false;
        transact(req, &dummy, ic);
    }

    /* Called when this input method is deactivated (focus leaves). */
    void deactivate(const fcitx::InputMethodEntry & /*entry*/,
                    fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        char req[64];
        snprintf(req, sizeof(req), "DEACTIVATE %p\n",
                 static_cast<void *>(ic));
        bool dummy = false;
        transact(req, &dummy, ic);
        /* Ensure any residual preedit decoration is removed. */
        ic->inputPanel().setClientPreedit(fcitx::Text{});
        ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }

    /* Called when the input context requests a composition reset. */
    void reset(const fcitx::InputMethodEntry & /*entry*/,
               fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        ic->inputPanel().setClientPreedit(fcitx::Text{});
        ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }
};

// ── Addon factory ─────────────────────────────────────────────────────────────

class ViImeFactory : public fcitx::AddonFactory {
public:
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
        return new ViImeEngine(manager);
    }
};

FCITX_ADDON_FACTORY(ViImeFactory)
