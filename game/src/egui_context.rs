use std::collections::HashMap;

use egui::epaint::Primitive;
use egui::{Context, FullOutput};
use egui_winit::EventResponse;
use winit::event::WindowEvent;
use winit::event_loop::EventLoopWindowTarget;

use jb_gfx::prelude::*;

pub struct EguiContext {
    pub egui_ctx: Context,
    pub egui_winit: egui_winit::State,
    pub stored_textures: HashMap<egui::TextureId, ImageHandle>,
    pub last_output: Option<FullOutput>,
}

impl EguiContext {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        Self {
            egui_ctx: Context::default(),
            egui_winit: egui_winit::State::new(event_loop),
            stored_textures: HashMap::default(),
            last_output: None,
        }
    }

    pub fn run(&mut self, window: &winit::window::Window, run_ui: impl FnOnce(&Context)) {
        let raw_input = self.egui_winit.take_egui_input(window);
        self.last_output = Some(self.egui_ctx.run(raw_input, run_ui));
        let output = self.egui_ctx.end_frame();
        self.egui_winit
            .handle_platform_output(window, &self.egui_ctx, output.platform_output);
    }

    pub fn paint(&mut self, renderer: &mut Renderer) {
        let full_output = self.last_output.take().unwrap();
        // Proces Texture Changes
        for (id, delta) in full_output.textures_delta.set.iter() {
            // TODO : Implement changing texture properties
            if self.stored_textures.contains_key(id) {
                continue;
            }

            let data: Vec<u8> = match &delta.image {
                egui::ImageData::Color(image) => {
                    assert_eq!(
                        image.width() * image.height(),
                        image.pixels.len(),
                        "Mismatch between texture size and texel count"
                    );
                    image
                        .pixels
                        .iter()
                        .flat_map(|color| color.to_array())
                        .collect()
                }
                egui::ImageData::Font(image) => image
                    .srgba_pixels(None)
                    .flat_map(|color| color.to_array())
                    .collect(),
            };

            let image = renderer.load_texture_from_bytes(
                &data,
                delta.image.width() as u32,
                delta.image.height() as u32,
                &ImageFormatType::Default,
                1,
            );
            self.stored_textures.insert(*id, image.unwrap());
        }

        // Paint
        let clipped_primitives = self.egui_ctx.tessellate(full_output.shapes);
        for prim in clipped_primitives.into_iter() {
            match prim.primitive {
                Primitive::Mesh(mesh) => {
                    let ui_verts = mesh
                        .vertices
                        .iter()
                        .map(|vert| UIVertex {
                            pos: vert.pos.into(),
                            uv: vert.uv.into(),
                            colour: vert
                                .color
                                .to_srgba_unmultiplied()
                                .map(|colour| colour as f32 / 255f32),
                        })
                        .collect();

                    let texture_id = {
                        if let Some(image) = self.stored_textures.get(&mesh.texture_id) {
                            *image
                        } else {
                            Default::default()
                        }
                    };

                    let ui_mesh = UIMesh {
                        indices: mesh.indices,
                        vertices: ui_verts,
                        texture_id,
                        scissor: (
                            prim.clip_rect.min.to_vec2().into(),
                            prim.clip_rect.max.to_vec2().into(),
                        ),
                    };
                    renderer.draw_ui(ui_mesh).unwrap();
                }
                Primitive::Callback(_) => {
                    todo!()
                }
            }
        }
    }

    pub fn on_event(&mut self, event: &WindowEvent) -> EventResponse {
        self.egui_winit.on_event(&self.egui_ctx, event)
    }
}
