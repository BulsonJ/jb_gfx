pub fn draw_timestamps(ui : &mut egui::Ui, timestamps: jb_gfx::renderer::TimeStamp) {
    ui.horizontal(|ui| {
        ui.label("Shadow Pass:");
        ui.label(format!("{:.6}", timestamps.shadow_pass.to_string()));
    });
    ui.horizontal(|ui| {
        ui.label("Deferred GBuffer:");
        ui.label(format!("{:.6}", timestamps.deferred_fill_pass.to_string()));
    });
    ui.horizontal(|ui| {
        ui.label("Deferred Lighting:");
        ui.label(format!(
            "{:.6}",
            timestamps.deferred_lighting_pass.to_string()
        ));
    });
    ui.horizontal(|ui| {
        ui.label("Forward Pass:");
        ui.label(format!("{:.6}", timestamps.forward_pass.to_string()));
    });
    ui.horizontal(|ui| {
        ui.label("Bloom Pass:");
        ui.label(format!("{:.6}", timestamps.bloom_pass.to_string()));
    });
    ui.horizontal(|ui| {
        ui.label("Combine Pass:");
        ui.label(format!("{:.6}", timestamps.combine_pass.to_string()));
    });
    ui.horizontal(|ui| {
        ui.label("UI Pass:");
        ui.label(format!("{:.6}", timestamps.ui_pass.to_string()));
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Frametime:");
        ui.label(format!("{:.6}", timestamps.total.to_string()));
    });
}