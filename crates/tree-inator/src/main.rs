#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Tree-inator")
            .with_inner_size([1500.0, 950.0])
            .with_min_inner_size([800.0, 500.0]),
        // the forest renderer needs buffers as large as the GPU allows
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(eframe::egui_wgpu::WgpuSetupCreateNew {
                device_descriptor: std::sync::Arc::new(|adapter| eframe::wgpu::DeviceDescriptor {
                    label: Some("tree-inator"),
                    required_limits: adapter.limits(),
                    ..Default::default()
                }),
                ..eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native("Tree-inator", options, Box::new(|cc| Ok(Box::new(app::TreeApp::new(cc)))))
}
