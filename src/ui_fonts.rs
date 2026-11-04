//! Lightweight system font fallback for Chinese annotations.
//! Loads on-demand from the OS; no font asset is bundled with the executable.
use eframe::egui;

pub fn install_system_cjk_fallback(ctx: &egui::Context) -> bool {
    let candidates = [
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "C:\\Windows\\Fonts\\msyh.ttc",
    ];
    for candidate in candidates {
        let Ok(bytes) = std::fs::read(candidate) else {
            continue;
        };
        let mut definitions = egui::FontDefinitions::default();
        definitions.font_data.insert(
            "rustbridge-system-cjk".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions
                .families
                .entry(family)
                .or_default()
                .push("rustbridge-system-cjk".into());
        }
        ctx.set_fonts(definitions);
        return true;
    }
    false
}
