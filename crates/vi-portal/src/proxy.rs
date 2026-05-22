/// Typed D-Bus proxy for the `org.freedesktop.portal.InputMethod` interface.
///
/// Use [`InputMethodPortalProxy::new`] to connect with the default service and
/// path, or [`InputMethodPortalProxy::builder`] to override them (e.g. in
/// tests).
#[zbus::proxy(
    interface = "org.freedesktop.portal.InputMethod",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
pub trait InputMethodPortal {
    /// Commit NFC-normalised text to the focused application.
    fn commit_string(&self, text: &str) -> zbus::Result<()>;

    /// Update or clear the in-progress preedit string.
    ///
    /// `cursor_begin` and `cursor_end` are Unicode scalar counts (not byte
    /// offsets) delimiting the highlighted region within `text`.
    #[zbus(name = "UpdatePreeditString")]
    fn set_preedit_string(
        &self,
        text: &str,
        cursor_begin: u32,
        cursor_end: u32,
    ) -> zbus::Result<()>;

    /// Forward a raw key event to the focused application.
    ///
    /// `keyval` and `keycode` are X11 keysym / hardware keycode values.
    /// `state` is a bitmask of X11 modifier flags.
    fn forward_key_event(&self, keyval: u32, keycode: u32, state: u32) -> zbus::Result<()>;
}
