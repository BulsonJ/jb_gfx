use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::hash::{Hash, Hasher};
use std::ops::BitOr;
use std::sync::Arc;

use anyhow::Result;
use ash::vk;
use ash::vk::{DescriptorSetLayout, Handle, ObjectType, PushConstantRange};
use log::{error, info, trace};
use slotmap::{new_key_type, SlotMap};

use crate::GraphicsDevice;

pub(crate) struct PipelineManager {
    device: Arc<GraphicsDevice>,
    shader_compiler: shaderc::Compiler,
    pipelines: SlotMap<PipelineHandle, Pipeline>,
    old_pipelines: Vec<vk::Pipeline>,
}

impl PipelineManager {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        let shader_compiler = shaderc::Compiler::new().unwrap();
        Self {
            device,
            shader_compiler,
            pipelines: SlotMap::default(),
            old_pipelines: Vec::default(),
        }
    }

    pub fn create_pipeline(&mut self, build_info: &PipelineCreateInfo) -> Result<PipelineHandle> {
        let pso = PipelineManager::create_pipeline_internal(
            &mut self.shader_compiler,
            &self.device,
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
            let object_name_string = String::from("Shader:")
                + build_info.vertex_shader.rsplit_once('/').unwrap().1
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

    pub fn reload_shaders(&mut self, device: &GraphicsDevice) {
        let mut new_pipelines = Vec::new();
        for (_, pipeline) in self.pipelines.iter() {
            new_pipelines.push(PipelineManager::create_pipeline_internal(
                &mut self.shader_compiler,
                device,
                &pipeline.create_info,
            ));
        }

        // Set ones that reloaded successfully
        for (i, (_, pipeline)) in self.pipelines.iter_mut().enumerate() {
            if let Ok(new_pipeline) = new_pipelines.get(i).unwrap() {
                self.old_pipelines.push(pipeline.pso);
                pipeline.pso = *new_pipeline;
            } else {
                error!(
                    "Unable to reload shader: [VERT:{}][FRAG:{}]",
                    pipeline.create_info.vertex_shader, pipeline.create_info.fragment_shader
                );
            }
        }

        let successful_reloads = new_pipelines
            .into_iter()
            .filter_map(|result| result.ok())
            .count();
        info!(
            "Reloaded {}/{} shaders!",
            successful_reloads,
            self.pipelines.len()
        );
    }

    pub fn deinit(&mut self) {
        for pipeline in self.old_pipelines.iter() {
            unsafe { self.device.vk_device.destroy_pipeline(*pipeline, None) };
        }
        for (_, pipeline) in self.pipelines.iter() {
            unsafe { self.device.vk_device.destroy_pipeline(pipeline.pso, None) };
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
    pub blend_op_color: vk::BlendOp,
    pub blend_op_alpha: vk::BlendOp,
    pub src_blend_factor_color: vk::BlendFactor,
    pub dst_blend_factor_color: vk::BlendFactor,
    pub src_blend_factor_alpha: vk::BlendFactor,
    pub dst_blend_factor_alpha: vk::BlendFactor,
}

impl Default for PipelineColorAttachment {
    fn default() -> Self {
        Self {
            format: vk::Format::default(),
            blend: false,
            blend_op_color: vk::BlendOp::ADD,
            blend_op_alpha: vk::BlendOp::ADD,
            src_blend_factor_color: vk::BlendFactor::ONE,
            dst_blend_factor_color: vk::BlendFactor::ONE,
            src_blend_factor_alpha: vk::BlendFactor::ONE,
            dst_blend_factor_alpha: vk::BlendFactor::ONE,
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
            .color_blend_op(attachment.blend_op_color)
            .src_color_blend_factor(attachment.src_blend_factor_color)
            .src_alpha_blend_factor(attachment.src_blend_factor_alpha)
            .alpha_blend_op(attachment.blend_op_alpha)
            .dst_color_blend_factor(attachment.dst_blend_factor_color)
            .dst_alpha_blend_factor(attachment.dst_blend_factor_alpha);
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

pub struct PipelineLayoutCache {
    device: Arc<ash::Device>,
    layout_cache: HashMap<PipelineLayoutInfo, vk::PipelineLayout>,
}

impl PipelineLayoutCache {
    pub fn new(device: Arc<ash::Device>) -> Self {
        Self {
            device,
            layout_cache: HashMap::default(),
        }
    }

    pub fn cleanup(&self) {
        for (_, set_layout) in self.layout_cache.iter() {
            unsafe { self.device.destroy_pipeline_layout(*set_layout, None) }
        }
    }

    pub fn create_pipeline_layout(
        &mut self,
        descriptor_sets: &[DescriptorSetLayout],
        push_constants: &[PushConstantRange],
    ) -> Result<vk::PipelineLayout> {
        let layout_info = PipelineLayoutInfo {
            descriptor_sets: Vec::from(descriptor_sets),
            push_constant_range: Vec::from(push_constants),
        };

        let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layout_info.descriptor_sets)
            .push_constant_ranges(&layout_info.push_constant_range);

        return if let Some(layout) = self.layout_cache.get(&layout_info) {
            Ok(*layout)
        } else {
            let layout = unsafe {
                self.device
                    .create_pipeline_layout(&pipeline_layout_create_info, None)
            }?;
            self.layout_cache.insert(layout_info, layout);
            Ok(layout)
        };
    }
}

struct PipelineLayoutInfo {
    descriptor_sets: Vec<vk::DescriptorSetLayout>,
    push_constant_range: Vec<vk::PushConstantRange>,
}

impl PartialEq<Self> for PipelineLayoutInfo {
    fn eq(&self, other: &Self) -> bool {
        if self.descriptor_sets.len() != other.descriptor_sets.len() {
            return false;
        }

        for (i, set) in self.descriptor_sets.iter().enumerate() {
            let other_set = other.descriptor_sets.get(i).unwrap();

            if other_set != set {
                return false;
            }
        }
        for (i, range) in self.push_constant_range.iter().enumerate() {
            let other_range = other.push_constant_range.get(i).unwrap();

            if other_range.stage_flags != range.stage_flags {
                return false;
            }
            if other_range.size != range.size {
                return false;
            }
            if other_range.offset != range.offset {
                return false;
            }
        }
        true
    }
}

impl Eq for PipelineLayoutInfo {}

impl Hash for PipelineLayoutInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.descriptor_sets.len().hash(state);

        for layout in self.descriptor_sets.iter() {
            let binding: i64 = layout.as_raw() as i64;
            binding.hash(state);
        }
        for push_constant in self.push_constant_range.iter() {
            let binding: i64 = (push_constant.offset as i64).bitor(
                ((push_constant.size as i64) << 8i64)
                    .bitor((push_constant.stage_flags.as_raw() << 16i64) as i64),
            );
            binding.hash(state);
        }
    }
}
