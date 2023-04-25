use cgmath::{Array, Deg, InnerSpace, Matrix4, Point3, Quaternion, Rotation3, Vector3};
use egui_winit::EventResponse;
use kira::manager::backend::cpal::CpalBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;

use game::application::{run_game, Application};
use game::components::{CameraComponent, LightComponent};
use game::editor::{Editor, EditorDependencies};
use game::egui_context::EguiContext;
use game::project::Project;
use game::{Camera, DirectionCamera, LookAtCamera};
use jb_gfx::device::ImageFormatType;
use jb_gfx::renderer::Renderer;
use jb_gfx::{Colour, DefaultCamera, Light};

fn main() {
    run_game::<EditorProject>()
}

struct EditorProject {
    lights: Vec<LightComponent>,
    cameras: Vec<CameraComponent>,
    egui: EguiContext,
    editor: Editor,
    audio_manager: AudioManager,
    background_music: StaticSoundData,
}

impl Project for EditorProject {
    fn new(app: &mut Application, event_loop: &EventLoop<()>) -> Self {
        // Load cube
        let texture = app
            .asset_manager
            .load_texture(
                &mut app.renderer,
                "assets/textures/light.png",
                &ImageFormatType::Default,
            )
            .unwrap();
        app.renderer.light_texture = Some(texture);
        // Load sponza
        {
            let models = app
                .asset_manager
                .load_gltf(&mut app.renderer, "assets/models/Sponza/glTF/Sponza.gltf")
                .unwrap();
            for model in models.iter() {
                let transform = from_transforms(
                    Vector3::new(0f32, -20f32, 0.0f32),
                    Quaternion::from_axis_angle(
                        Vector3::new(0f32, 1f32, 0.0f32).normalize(),
                        Deg(180f32),
                    ),
                    Vector3::from_value(0.1f32),
                );
                Vector3::from_value(0.1f32);
                for submesh in model.mesh.submeshes.iter() {
                    let handle = app
                        .renderer
                        .add_render_model(submesh.mesh, submesh.material_instance);
                    app.renderer
                        .set_render_model_transform(handle, transform)
                        .unwrap();
                }
            }
        }
        // Load helmet
        {
            let models = app
                .asset_manager
                .load_gltf(
                    &mut app.renderer,
                    "assets/models/DamagedHelmet/glTF/DamagedHelmet.gltf",
                )
                .unwrap();
            for model in models.iter() {
                let transform = from_transforms(
                    Vector3::new(10f32, 0f32, 0.0f32),
                    Quaternion::from_axis_angle(
                        Vector3::new(1f32, 0f32, 0.0f32).normalize(),
                        Deg(100f32),
                    ) * Quaternion::from_axis_angle(
                        Vector3::new(0f32, 0f32, 1.0f32).normalize(),
                        Deg(60f32),
                    ),
                    Vector3::from_value(6f32),
                );
                Vector3::from_value(0.1f32);
                for submesh in model.mesh.submeshes.iter() {
                    let handle = app
                        .renderer
                        .add_render_model(submesh.mesh, submesh.material_instance);
                    app.renderer
                        .set_render_model_transform(handle, transform)
                        .unwrap();
                }
            }
        }
        app.renderer.clear_colour = Colour::new(0.0, 0.1, 0.3);

        let (lights, cameras) = setup_scene(
            &mut app.renderer,
            (
                app.window.inner_size().width,
                app.window.inner_size().height,
            ),
        );

        let audio_manager =
            AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).unwrap();
        let background_music =
            StaticSoundData::from_file("assets/sounds/prelude.ogg", StaticSoundSettings::default())
                .unwrap();

        let egui = EguiContext::new(event_loop);
        let editor = Editor::new();

        Self {
            egui,
            editor,
            lights,
            cameras,
            audio_manager,
            background_music,
        }
    }

    fn update(&mut self, ctx: &mut Application) {
        for (i, component) in self.lights.iter_mut().enumerate() {
            let position = 10f32 + ((i as f32 + 3f32 * ctx.time_passed).sin() * 5f32);
            component.light.position.x = position;
        }
        // Update render objects & then render
        update_renderer_object_states(&mut ctx.renderer, &self.lights);
        let camera = &self.cameras[self.editor.camera_panel().selected_camera_index()];
        match &camera.camera {
            Camera::Directional(camera) => {
                ctx.renderer.set_camera(camera);
            }
            Camera::LookAt(camera) => {
                ctx.renderer.set_camera(camera);
            }
        }
    }

    fn draw(&mut self, app: &mut Application) {
        self.egui.run(&app.window, |ctx| {
            self.editor.run(
                ctx,
                &mut EditorDependencies {
                    input: &app.input,
                    renderer: &mut app.renderer,
                    audio_manager: &mut self.audio_manager,
                    background_music: &mut self.background_music,
                    cameras: &mut self.cameras,
                    lights: &mut self.lights,
                },
            )
        });
        self.egui.paint(&mut app.renderer);
    }

    fn on_window_event(&mut self, event: &WindowEvent) -> EventResponse {
        self.egui.on_event(event)
    }
}

#[profiling::function]
fn setup_scene(
    renderer: &mut Renderer,
    screen_size: (u32, u32),
) -> (Vec<LightComponent>, Vec<CameraComponent>) {
    let initial_lights = vec![
        Light::new(
            Point3::new(10.0f32, -5.0f32, -16.0f32),
            Vector3::new(5.0f32, 0.0f32, 0.0f32),
        ),
        Light::new(
            Point3::new(-10.0f32, 5.0f32, 16.0f32),
            Vector3::new(0.0f32, 5.0f32, 0.0f32),
        ),
        Light::new(
            Point3::new(10.0f32, 5.0f32, -16.0f32),
            Vector3::new(5.0f32, 5.0f32, 5.0f32),
        ),
        Light::new(
            Point3::new(-10.0f32, -5.0f32, 16.0f32),
            Vector3::new(5.0f32, 5.0f32, 5.0f32),
        ),
    ];

    let light_components = vec![
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(0).unwrap())
                .unwrap(),
            light: *initial_lights.get(0).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(1).unwrap())
                .unwrap(),
            light: *initial_lights.get(1).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(2).unwrap())
                .unwrap(),
            light: *initial_lights.get(2).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(3).unwrap())
                .unwrap(),
            light: *initial_lights.get(3).unwrap(),
        },
    ];

    let cameras = vec![
        {
            let camera = Camera::LookAt(LookAtCamera {
                position: (-8.0, 0.0, 0.0).into(),
                target: (1.0, 0.0, 0.0).into(),
                aspect: screen_size.0 as f32 / screen_size.1 as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            });
            CameraComponent { camera }
        },
        {
            let camera = Camera::Directional(DirectionCamera {
                position: (-50.0, 0.0, 20.0).into(),
                direction: (1.0, 0.25, -0.5).into(),
                aspect: screen_size.0 as f32 / screen_size.1 as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            });
            CameraComponent { camera }
        },
        {
            CameraComponent {
                camera: Camera::Directional(DirectionCamera {
                    position: (-75.0, 100.0, 20.0).into(),
                    direction: (1.0, -0.75, -0.5).into(),
                    aspect: screen_size.0 as f32 / screen_size.1 as f32,
                    fovy: 90.0,
                    znear: 0.1,
                    zfar: 4000.0,
                }),
            }
        },
    ];

    (light_components, cameras)
}

#[profiling::function]
fn update_renderer_object_states(renderer: &mut Renderer, light_components: &[LightComponent]) {
    for component in light_components.iter() {
        renderer
            .set_light(component.handle, &component.light)
            .unwrap();
    }
}

#[profiling::function]
pub fn from_transforms(
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
    size: Vector3<f32>,
) -> Matrix4<f32> {
    let translation = Matrix4::from_translation(position);
    let rotation = Matrix4::from(rotation);
    let scale = Matrix4::from_nonuniform_scale(size.x, size.y, size.z);

    translation * rotation * scale
}
