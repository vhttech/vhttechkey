.DEFAULT_GOAL := help

CARGO        := cargo
RELEASE_FLAG :=
TARGET       := x86_64-unknown-linux-gnu

# Pass RELEASE=1 to build in release mode: make build RELEASE=1
ifdef RELEASE
RELEASE_FLAG := --release
endif

##@ Build

.PHONY: build
build: ## Build toàn bộ workspace (debug)
	$(CARGO) build --workspace $(RELEASE_FLAG)

.PHONY: build-release
build-release: ## Build toàn bộ workspace (release, tối ưu hoá)
	$(CARGO) build --workspace --release

.PHONY: build-daemon
build-daemon: ## Build chỉ vi-daemon
	$(CARGO) build -p vi-daemon $(RELEASE_FLAG)

.PHONY: build-tools
build-tools: ## Build chỉ vi-tools
	$(CARGO) build -p vi-tools $(RELEASE_FLAG)

##@ Kiểm tra & Lint

.PHONY: check
check: ## Kiểm tra biên dịch mà không tạo file output
	$(CARGO) check --workspace

.PHONY: clippy
clippy: ## Chạy Clippy (linter) trên toàn workspace
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format code bằng rustfmt
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Kiểm tra format mà không thay đổi file
	$(CARGO) fmt --all -- --check

.PHONY: deny
deny: ## Kiểm tra dependency (licenses, advisories) với cargo-deny
	$(CARGO) deny check

##@ Test

.PHONY: test
test: ## Chạy toàn bộ test suite
	$(CARGO) test --workspace

.PHONY: test-verbose
test-verbose: ## Chạy test với output chi tiết
	$(CARGO) test --workspace -- --nocapture

.PHONY: test-crate
test-crate: ## Chạy test cho một crate cụ thể: make test-crate CRATE=vi-core
	$(CARGO) test -p $(CRATE)

.PHONY: test-ibus
test-ibus: ## Chạy IBus integration tests trong D-Bus session riêng (dbus-run-session)
	dbus-run-session -- $(CARGO) test -p vi-testing --test integration_ibus

.PHONY: bench
bench: ## Chạy benchmarks
	$(CARGO) bench --workspace

##@ Fuzz

.PHONY: fuzz-list
fuzz-list: ## Liệt kê các fuzz target
	$(CARGO) fuzz list --manifest-path fuzz/Cargo.toml

.PHONY: fuzz
fuzz: ## Chạy fuzz target: make fuzz TARGET=<tên-target>
	$(CARGO) fuzz run --manifest-path fuzz/Cargo.toml $(TARGET)

##@ Đóng gói phát hành

.PHONY: deb
deb: build-release ## Tạo file .deb (Ubuntu/Debian) → dist/vhttechkey_*.deb
	bash packaging/build-deb.sh

.PHONY: rpm
rpm: build-release ## Tạo file .rpm (Fedora/RHEL) → dist/vhttechkey-*.rpm
	bash packaging/build-rpm.sh

.PHONY: appimage
appimage: build-release ## Tạo AppImage installer → dist/vhttechkey-installer-linux-x86_64.AppImage
	bash packaging/build-appimage.sh

.PHONY: packages
packages: deb rpm appimage ## Tạo tất cả gói phát hành (.deb + .rpm + AppImage)

##@ Dọn dẹp

.PHONY: clean
clean: ## Xoá toàn bộ artifacts build và thư mục dist
	$(CARGO) clean
	rm -rf dist/

.PHONY: clean-doc
clean-doc: ## Xoá thư mục docs được tạo ra
	rm -rf target/doc

##@ Tài liệu

.PHONY: doc
doc: ## Sinh tài liệu (mở trong trình duyệt)
	$(CARGO) doc --workspace --no-deps --open

.PHONY: doc-build
doc-build: ## Sinh tài liệu (không mở trình duyệt)
	$(CARGO) doc --workspace --no-deps

##@ CI / Kiểm tra toàn diện

.PHONY: ci
ci: fmt-check clippy test ## Chạy toàn bộ kiểm tra CI (fmt + clippy + test)

.PHONY: pre-commit
pre-commit: fmt clippy test ## Chạy trước khi commit (fmt + clippy + test)

##@ Tiện ích

.PHONY: update
update: ## Cập nhật dependencies lên phiên bản mới nhất
	$(CARGO) update

.PHONY: outdated
outdated: ## Kiểm tra dependencies lỗi thời (cần cargo-outdated)
	$(CARGO) outdated --workspace

.PHONY: tree
tree: ## In cây dependency
	$(CARGO) tree --workspace

.PHONY: install-daemon
install-daemon: build-release ## Cài vi-daemon vào ~/.cargo/bin
	$(CARGO) install --path crates/vi-daemon

.PHONY: install
install: build-release ## Cài toàn bộ vào hệ thống (cần sudo)
	sudo packaging/scripts/install.sh

.PHONY: uninstall
uninstall: ## Gỡ cài đặt khỏi hệ thống (cần sudo)
	sudo packaging/scripts/uninstall.sh

.PHONY: perf-check
perf-check: ## Chạy script kiểm tra regression hiệu năng
	python3 scripts/check_perf_regression.py

.PHONY: bench-check
bench-check: ## Chạy script kiểm tra regression benchmark
	python3 scripts/check_bench_regression.py

##@ Trợ giúp

.PHONY: help
help: ## Hiển thị danh sách lệnh này
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
	/^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
