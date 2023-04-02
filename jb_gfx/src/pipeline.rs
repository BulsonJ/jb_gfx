use std::ffi::{CStr, CString};
use std::fs;

use crate::device::GraphicsDevice;
use ash::vk;
use ash::vk::{DebugUtilsObjectNameInfoEXT, Handle, ObjectType};
use slotmap::{new_key_type, SlotMap};

pub(crate) struct PipelineManager {
    shader_compiler: shaderc::Compiler,
    pipelines: SlotMap<PipelineHandle, Pipeline>,
}

impl PipelineManager {
    pub fn new() -> Self {
        let shader_compiler = shaderc::Compiler::new().unwrap();
        Self {
            shader_compiler,
            pipelines: SlotMap::default(),
        }
    }

    pub fn create_pipeline(
        &mut self,
        device: &mut GraphicsDevice,
        build_info: &PipelineCreateInfo,
    ) -> PipelineHandle {
        let pso = PipelineManager::create_pipeline_internal(
            &mut self.shader_compiler,
            device,
            build_info.clone(),
        );
        self.pipelines.insert(Pipeline {
            pso,
            create_info: build_info.clone(),
        })
    }

    fn create_pipeline_internal(
        shader_compiler: &mut shaderc::Compiler,
        device: &mut crate::device::GraphicsDevice,
        build_info: PipelineCreateInfo,
    ) -> vk::Pipeline {
        let vertex_file = fs::read_to_string(&build_info.vertex_shader).unwrap();
        let frag_file = fs::read_to_string(&build_info.fragment_shader).unwrap();

        let vert_binary = shader_compiler
            .compile_into_spirv(
                &vertex_file,
                shaderc::ShaderKind::Vertex,
                &build_info.vertex_shader,
                "main",
                None,
            )
            .unwrap();

        let frag_binary = shader_compiler
            .compile_into_spirv(
                &frag_file,
                shaderc::ShaderKind::Fragment,
                &build_info.fragment_shader,
                "main",
                None,
            )
            .unwrap();

        let vertex_shader = load_shader_module(&device.vk_device, vert_binary.as_binary()).unwrap();

        let vertex_stage_info = vk::PipelineShaderStageCreateInfo::builder()
            .name(unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") })
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .build();

        let fragment_shader =
            load_shader_module(&device.vk_device, frag_binary.as_binary()).unwrap();

        let fragment_stage_info = vk::PipelineShaderStageCreateInfo::builder()
            .name(unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") })
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .build();

        let info = PipelineBuildInfo {
            shader_stages: vec![vertex_stage_info, fragment_stage_info],
            vertex_input_state: build_info.vertex_input_state,
            color_attachment_formats: build_info.color_attachment_formats,
            depth_attachment_format: build_info.depth_attachment_format,
            depth_stencil_state: build_info.depth_stencil_state,
            pipeline_layout: build_info.pipeline_layout,
        };

        let pipeline = build_pipeline(&mut device.vk_device, info);

        let object_name_string = String::from(build_info.vertex_shader.rsplit_once('/').unwrap().1)
            + " "
            + build_info.fragment_shader.rsplit_once('/').unwrap().1;
        let object_name = CString::new(object_name_string).unwrap();
        let pipeline_debug_info = DebugUtilsObjectNameInfoEXT::builder()
            .object_type(ObjectType::PIPELINE)
            .object_handle(pipeline.as_raw())
            .object_name(object_name.as_ref());

        unsafe {
            device
                .debug_utils_loader
                .set_debug_utils_object_name(device.vk_device.handle(), &pipeline_debug_info)
                .expect("Named object");
        }

        unsafe {
            device.vk_device.destroy_shader_module(vertex_shader, None);
            device
                .vk_device
                .destroy_shader_module(fragment_shader, None);
        }

        pipeline
    }

    pub fn get_pipeline(&mut self, handle: PipelineHandle) -> vk::Pipeline {
        self.pipelines.get(handle).unwrap().pso
    }

    pub fn reload_shaders(&mut self, device: &mut GraphicsDevice) {
        for (_, pipeline) in self.pipelines.iter_mut() {
            pipeline.pso = PipelineManager::create_pipeline_internal(
                &mut self.shader_compiler,
                device,
                pipeline.create_info.clone(),
            )
        }
    }

    pub fn deinit(&mut self, device: &mut ash::Device) {
        for (_, pipeline) in self.pipelines.iter_mut() {
            unsafe { device.destroy_pipeline(pipeline.pso, None) };
        }
    }
}

new_key_type! {
    pub(crate) struct PipelineHandle;
}

struct Pipeline {
    pso: vk::Pipeline,
    create_info: PipelineCreateInfo,
}

#[derive(Clone)]
pub struct PipelineCreateInfo {
    pub pipeline_layout: vk::PipelineLayout,
    pub vertex_shader: String,
    pub fragment_shader: String,
    pub vertex_input_state: vk::PipelineVertexInputStateCreateInfo,
    pub color_attachment_formats: Vec<vk::Format>,
    pub depth_attachment_format: Option<vk::Format>,
    pub depth_stencil_state: vk::PipelineDepthStencilStateCreateInfo,
}

pub struct PipelineBuildInfo {
    pub shader_stages: Vec<vk::PipelineShaderStageCreateInfo>,
    pub vertex_input_state: vk::PipelineVertexInputStateCreateInfo,
    pub color_attachment_formats: Vec<vk::Format>,
    pub depth_attachment_format: Option<vk::Format>,
    pub depth_stencil_state: vk::PipelineDepthStencilStateCreateInfo,
    pub pipeline_layout: vk::PipelineLayout,
}

pub fn build_pipeline(device: &mut ash::Device, build_info: PipelineBuildInfo) -> vk::Pipeline {
    // Defaults

    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1);

    let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::builder()
        .blend_enable(false)
        .color_write_mask(vk::ColorComponentFlags::RGBA);

    let attachments = [*color_blend_attachment_state];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&attachments);

    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let tess_state = vk::PipelineTessellationStateCreateInfo::builder();

    let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .sample_shading_enable(false)
        .min_sample_shading(1.0f32)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);

    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false)
        .depth_bias_constant_factor(0.0f32)
        .depth_bias_clamp(0.0f32)
        .depth_bias_slope_factor(0.0f32)
        .line_width(1.0f32);

    let mut dynamic_rendering_info = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(&build_info.color_attachment_formats);
    // Ignore depth format for now .depth_attachment_format(build_info.depth_attachment_format);

    let pso_create_info = vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut dynamic_rendering_info)
        .stages(&build_info.shader_stages)
        .vertex_input_state(&build_info.vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .tessellation_state(&tess_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .depth_stencil_state(&build_info.depth_stencil_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(build_info.pipeline_layout);

    let create_info = [*pso_create_info];
    let pso =
        unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &create_info, None) };

    let pipeline_object = *pso.unwrap().get(0usize).unwrap();
    pipeline_object
}

pub fn load_shader_module(device: &ash::Device, code: &[u32]) -> Option<vk::ShaderModule> {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code);

    Some(unsafe { device.create_shader_module(&create_info, None) }.unwrap())
}
