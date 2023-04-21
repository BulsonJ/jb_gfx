use std::ffi::{CStr, CString};
use std::fs;

use anyhow::Result;
use ash::vk;
use ash::vk::{
    DescriptorSetLayout, Handle, ObjectType, PushConstantRange,
};
use log::trace;
use slotmap::{new_key_type, SlotMap};

use crate::device::GraphicsDevice;

pub(crate) struct PipelineManager {
    shader_compiler: shaderc::Compiler,
    pipelines: SlotMap<PipelineHandle, Pipeline>,
    pipeline_layouts: Vec<vk::PipelineLayout>,
}

impl PipelineManager {
    pub fn new() -> Self {
        let shader_compiler = shaderc::Compiler::new().unwrap();
        Self {
            shader_compiler,
            pipelines: SlotMap::default(),
            pipeline_layouts: Vec::default(),
        }
    }

    pub fn create_pipeline_layout(
        &mut self,
        device: &GraphicsDevice,
        descriptor_sets: &[DescriptorSetLayout],
        push_constants: &[PushConstantRange],
    ) -> Result<vk::PipelineLayout> {
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&descriptor_sets)
            .push_constant_ranges(&push_constants);

        let layout = unsafe {
            device
                .vk_device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }?;
        self.pipeline_layouts.push(layout);

        Ok(layout)
    }

    pub fn create_pipeline(
        &mut self,
        device: &GraphicsDevice,
        build_info: &PipelineCreateInfo,
    ) -> Result<PipelineHandle> {
        let pso = PipelineManager::create_pipeline_internal(
            &mut self.shader_compiler,
            device,
            build_info,
        )?;
        Ok(self.pipelines.insert(Pipeline {
            pso,
            create_info: build_info.clone(),
        }))
    }

    fn create_pipeline_internal(
        shader_compiler: &mut shaderc::Compiler,
        device: &GraphicsDevice,
        build_info: &PipelineCreateInfo,
    ) -> Result<vk::Pipeline> {
        let vertex_file = fs::read_to_string(&build_info.vertex_shader)?;
        let frag_file = fs::read_to_string(&build_info.fragment_shader)?;

        let mut options = shaderc::CompileOptions::new().unwrap();
        options.set_include_callback(include_resolve_callback);

        let vert_binary = shader_compiler.compile_into_spirv(
            &vertex_file,
            shaderc::ShaderKind::Vertex,
            &build_info.vertex_shader,
            "main",
            Some(&options),
        )?;

        let frag_binary = shader_compiler.compile_into_spirv(
            &frag_file,
            shaderc::ShaderKind::Fragment,
            &build_info.fragment_shader,
            "main",
            Some(&options),
        )?;

        let vertex_shader = load_shader_module(&device.vk_device, vert_binary.as_binary())?;

        let vertex_stage_info = vk::PipelineShaderStageCreateInfo::builder()
            .name(unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") })
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader)
            .build();

        let fragment_shader = load_shader_module(&device.vk_device, frag_binary.as_binary())?;

        let fragment_stage_info = vk::PipelineShaderStageCreateInfo::builder()
            .name(unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") })
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader)
            .build();

        let info = PipelineBuildInfo {
            shader_stages: vec![vertex_stage_info, fragment_stage_info],
            vertex_input_state: build_info.vertex_input_state.clone(),
            color_attachment_formats: build_info.color_attachment_formats.clone(),
            depth_attachment_format: build_info.depth_attachment_format,
            depth_stencil_state: build_info.depth_stencil_state,
            pipeline_layout: build_info.pipeline_layout,
            cull_mode: build_info.cull_mode,
        };

        let pipeline = build_pipeline(&device.vk_device, info);

        {
            let object_name_string =
                String::from(build_info.vertex_shader.rsplit_once('/').unwrap().1)
                    + " "
                    + build_info.fragment_shader.rsplit_once('/').unwrap().1;
            device.set_vulkan_debug_name(
                pipeline.as_raw(),
                ObjectType::PIPELINE,
                &object_name_string,
            )?;
        }

        unsafe {
            device.vk_device.destroy_shader_module(vertex_shader, None);
            device
                .vk_device
                .destroy_shader_module(fragment_shader, None);
        }

        Ok(pipeline)
    }

    pub fn get_pipeline(&self, handle: PipelineHandle) -> vk::Pipeline {
        self.pipelines.get(handle).unwrap().pso
    }

    pub fn reload_shaders(&mut self, device: &mut GraphicsDevice) -> Result<()> {
        for (_, pipeline) in self.pipelines.iter_mut() {
            pipeline.pso = PipelineManager::create_pipeline_internal(
                &mut self.shader_compiler,
                device,
                &pipeline.create_info,
            )?
        }
        Ok(())
    }

    pub fn deinit(&mut self, device: &ash::Device) {
        for (_, pipeline) in self.pipelines.iter_mut() {
            unsafe { device.destroy_pipeline(pipeline.pso, None) };
        }
        for layout in self.pipeline_layouts.iter_mut() {
            unsafe { device.destroy_pipeline_layout(*layout, None) };
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
    pub vertex_input_state: VertexInputDescription,
    pub color_attachment_formats: Vec<PipelineColorAttachment>,
    pub depth_attachment_format: Option<vk::Format>,
    pub depth_stencil_state: vk::PipelineDepthStencilStateCreateInfo,
    pub cull_mode: vk::CullModeFlags,
}

pub struct PipelineBuildInfo {
    pub shader_stages: Vec<vk::PipelineShaderStageCreateInfo>,
    pub vertex_input_state: VertexInputDescription,
    pub color_attachment_formats: Vec<PipelineColorAttachment>,
    pub depth_attachment_format: Option<vk::Format>,
    pub depth_stencil_state: vk::PipelineDepthStencilStateCreateInfo,
    pub pipeline_layout: vk::PipelineLayout,
    pub cull_mode: vk::CullModeFlags,
}

#[derive(Clone)]
pub struct PipelineColorAttachment {
    pub format: vk::Format,
    pub blend: bool,
    pub src_blend_factor_color: vk::BlendFactor,
    pub dst_blend_factor_color: vk::BlendFactor,
}

impl Default for PipelineColorAttachment {
    fn default() -> Self {
        Self {
            format: vk::Format::default(),
            blend: false,
            src_blend_factor_color: vk::BlendFactor::ONE,
            dst_blend_factor_color: vk::BlendFactor::ONE,
        }
    }
}

pub fn build_pipeline(device: &ash::Device, build_info: PipelineBuildInfo) -> vk::Pipeline {
    // Defaults

    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1);

    let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

    let mut attachments = Vec::new();
    for attachment in build_info.color_attachment_formats.iter() {
        let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(attachment.blend)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .src_color_blend_factor(attachment.src_blend_factor_color)
            //.src_alpha_blend_factor(attachment.blend_factor_alpha);
            .dst_color_blend_factor(attachment.dst_blend_factor_color);
        //.dst_alpha_blend_factor(attachment.blend_factor_alpha);
        attachments.push(*color_blend_attachment_state);
    }

    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&attachments);

    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&build_info.vertex_input_state.bindings)
        .vertex_attribute_descriptions(&build_info.vertex_input_state.attributes);

    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let tess_state = vk::PipelineTessellationStateCreateInfo::builder();

    let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(build_info.cull_mode)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false)
        .depth_bias_constant_factor(0.0f32)
        .depth_bias_clamp(0.0f32)
        .depth_bias_slope_factor(0.0f32)
        .line_width(1.0f32);

    let color_attachment_formats: Vec<vk::Format> = build_info
        .color_attachment_formats
        .iter()
        .map(|attachment| attachment.format)
        .collect();
    let mut dynamic_rendering_info = {
        if let Some(depth_format) = build_info.depth_attachment_format {
            vk::PipelineRenderingCreateInfo::builder()
                .color_attachment_formats(&color_attachment_formats)
                .depth_attachment_format(depth_format)
        } else {
            vk::PipelineRenderingCreateInfo::builder()
                .color_attachment_formats(&color_attachment_formats)
        }
    };

    let pso_create_info = vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut dynamic_rendering_info)
        .stages(&build_info.shader_stages)
        .vertex_input_state(&vertex_input_state)
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

pub fn load_shader_module(device: &ash::Device, code: &[u32]) -> Result<vk::ShaderModule> {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code);

    Ok(unsafe { device.create_shader_module(&create_info, None) }?)
}

fn include_resolve_callback(
    requested_file_name: &str,
    include_type: shaderc::IncludeType,
    source_file_name: &str,
    include_depth: usize,
) -> shaderc::IncludeCallbackResult {
    trace!("Attempting to resolve library: {}", requested_file_name);
    trace!("Include Type: {:?}", include_type);
    trace!("Directive source file: {}", source_file_name);
    trace!("Current library depth: {}", include_depth);

    let content = fs::read_to_string(requested_file_name).unwrap();

    Ok(shaderc::ResolvedInclude {
        resolved_name: requested_file_name.to_string(),
        content,
    })
}

#[derive(Clone)]
pub struct VertexInputDescription {
    pub bindings: Vec<vk::VertexInputBindingDescription>,
    pub attributes: Vec<vk::VertexInputAttributeDescription>,
}
