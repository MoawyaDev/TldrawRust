fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([850.0, 540.0])
            .with_title("GPUII — Infinite Canvas"),
        ..Default::default()
    };
    eframe::run_native(
        "GPUII",
        options,
        Box::new(|_creation_context| Ok(Box::<gpuii::app::GpuiiApp>::default())),
    )
}
