use gltf::Gltf;
use crate::renderer::{MeshHandle, Renderer};

pub struct AssetManager {}

impl AssetManager {
    pub fn load_model(renderer: &mut Renderer, file: &str) -> Vec<Model> {
        let gltf_model = Gltf::open(file).unwrap();

        let models = Vec::new();

        for scene in gltf_model.scenes() {
            for node in scene.nodes() {
                if let Some(mesh) = node.mesh() {
                    for prim in mesh.primitives() {
                        println!("Primitive attributes: {}", prim.attributes().count());

                    }
                }
            }
        }

        models
    }

}

pub struct Model{
    pub mesh: MeshHandle,
}
