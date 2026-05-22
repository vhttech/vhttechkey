fn main() -> eframe::Result<()> {
    let options = vi_ui::main_window_options();
    let mut last_err = None;

    // wgpu trước: tránh lỗi GLX BadValue trên X11 không có GL thật (VNC, Xephyr, X forwarding…).
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let mut opts = options.clone();
        opts.renderer = renderer;
        match eframe::run_native("vhttechkey-ui", opts, Box::new(create_app)) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if renderer == eframe::Renderer::Wgpu {
                    eprintln!("vi-ui: wgpu renderer failed ({e}), trying OpenGL (glow)...");
                }
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        eframe::Error::AppCreation(Box::new(std::io::Error::other("no renderer available")))
    }))
}

fn create_app(
    cc: &eframe::CreationContext<'_>,
) -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "HackNerdFont".into(),
        egui::FontData::from_static(include_bytes!(
            "../../../data/fonts/Hack/HackNerdFont-Regular.ttf"
        )),
    );

    fonts.font_data.insert(
        "NotoSans".into(),
        egui::FontData::from_static(include_bytes!("../../../data/fonts/NotoSans-Regular.ttf")),
    );

    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    proportional.insert(0, "HackNerdFont".into());
    proportional.push("NotoSans".into());

    let monospace = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    monospace.insert(0, "HackNerdFont".into());
    monospace.push("NotoSans".into());

    cc.egui_ctx.set_fonts(fonts);
    Ok(Box::new(vi_ui::ViUiApp::new(cc)))
}
