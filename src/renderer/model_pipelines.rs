use super::{
    draw_model, AnimatedVertex, DynamicBuffer, RenderContext, Vertex, DEPTH_FORMAT, DISPLAY_FORMAT,
};
use crate::assets::{AnimatedModel, Assets, Model};
use std::sync::Arc;
use ultraviolet::{Mat4, Vec4};
use wgpu::util::DeviceExt;

pub struct ModelPipelines {
    identity_instance_buffer: wgpu::Buffer,
    model_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    animated_pipeline: wgpu::RenderPipeline,
    transparent_animated_pipeline: wgpu::RenderPipeline,
    main_bind_group: Arc<wgpu::BindGroup>,
}

impl ModelPipelines {
    pub fn new(context: &RenderContext, assets: &Assets) -> Self {
        let vs = wgpu::include_spirv!("../../shaders/compiled/shader.vert.spv");
        let vs_module = context.device.create_shader_module(vs);

        let fs = wgpu::include_spirv!("../../shaders/compiled/shader.frag.spv");
        let fs_module = context.device.create_shader_module(fs);

        let fs_transparent = wgpu::include_spirv!("../../shaders/compiled/transparent.frag.spv");
        let fs_transparent_module = context.device.create_shader_module(fs_transparent);

        let vs_animated = wgpu::include_spirv!("../../shaders/compiled/animated.vert.spv");
        let vs_animated_module = context.device.create_shader_module(vs_animated);

        let model_pipeline = create_render_pipeline(
            &context.device,
            &[
                &context.main_bind_group_layout,
                &assets.texture_bind_group_layout,
            ],
            "Cheese model pipeline",
            wgpu::PrimitiveTopology::TriangleList,
            &vs_module,
            &fs_module,
            false,
        );

        let line_pipeline = create_render_pipeline(
            &context.device,
            &[
                &context.main_bind_group_layout,
                &assets.texture_bind_group_layout,
            ],
            "Cheese line pipeline",
            wgpu::PrimitiveTopology::LineList,
            &vs_module,
            &fs_module,
            false,
        );

        let animated_pipeline = create_animated_pipeline(
            &context.device,
            &[
                &context.main_bind_group_layout,
                &assets.texture_bind_group_layout,
                &context.joint_bind_group_layout,
            ],
            &vs_animated_module,
            &fs_module,
            false,
        );

        let transparent_animated_pipeline = create_animated_pipeline(
            &context.device,
            &[
                &context.main_bind_group_layout,
                &assets.texture_bind_group_layout,
                &context.joint_bind_group_layout,
            ],
            &vs_animated_module,
            &fs_transparent_module,
            true,
        );

        let identity_instance_buffer =
            context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Cheese identity instance buffer"),
                    contents: bytemuck::bytes_of(&ModelInstance {
                        transform: Mat4::identity(),
                        flat_colour: Vec4::one(),
                    }),
                    usage: wgpu::BufferUsage::VERTEX,
                });

        Self {
            identity_instance_buffer,
            model_pipeline,
            line_pipeline,
            animated_pipeline,
            transparent_animated_pipeline,
            main_bind_group: context.main_bind_group.clone(),
        }
    }

    pub fn render_animated<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instances: &'a DynamicBuffer<ModelInstance>,
        texture: &'a wgpu::BindGroup,
        model: &'a AnimatedModel,
        joints: &'a wgpu::BindGroup,
    ) {
        if let Some((slice, num)) = instances.get() {
            render_pass.set_pipeline(&self.animated_pipeline);
            render_pass.set_bind_group(0, &self.main_bind_group, &[]);
            render_pass.set_bind_group(1, texture, &[]);
            render_pass.set_bind_group(2, joints, &[]);

            render_pass.set_vertex_buffer(0, model.vertices.slice(..));
            render_pass.set_vertex_buffer(1, slice);
            render_pass.set_index_buffer(model.indices.slice(..));
            render_pass.draw_indexed(0..model.num_indices, 0, 0..num);
        }
    }

    pub fn render_transparent_animated<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instances: &'a DynamicBuffer<ModelInstance>,
        dummy_texture: &'a wgpu::BindGroup,
        model: &'a AnimatedModel,
        joints: &'a wgpu::BindGroup,
    ) {
        if let Some((slice, num)) = instances.get() {
            render_pass.set_pipeline(&self.transparent_animated_pipeline);
            render_pass.set_bind_group(0, &self.main_bind_group, &[]);
            // Needed for bind group reasons
            // (basically because I don't want to have 2 animation vertex shaders)
            render_pass.set_bind_group(1, dummy_texture, &[]);
            render_pass.set_bind_group(2, joints, &[]);

            render_pass.set_vertex_buffer(0, model.vertices.slice(..));
            render_pass.set_vertex_buffer(1, slice);
            render_pass.set_index_buffer(model.indices.slice(..));
            render_pass.draw_indexed(0..model.num_indices, 0, 0..num);
        }
    }

    pub fn render_single<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        texture: &'a wgpu::BindGroup,
        model: &'a Model,
    ) {
        render_pass.set_pipeline(&self.model_pipeline);
        render_pass.set_bind_group(0, &self.main_bind_group, &[]);
        render_pass.set_bind_group(1, texture, &[]);
        draw_model(
            render_pass,
            model,
            self.identity_instance_buffer.slice(..),
            1,
        );
    }

    pub fn render_instanced<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instances: &'a DynamicBuffer<ModelInstance>,
        texture: &'a wgpu::BindGroup,
        model: &'a Model,
    ) {
        if let Some((slice, num)) = instances.get() {
            render_pass.set_pipeline(&self.model_pipeline);
            render_pass.set_bind_group(0, &self.main_bind_group, &[]);
            render_pass.set_bind_group(1, texture, &[]);
            draw_model(render_pass, model, slice, num);
        }
    }

    pub fn render_lines<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        lines: &'a DynamicBuffer<Vertex>,
        texture: &'a wgpu::BindGroup,
    ) {
        if let Some((slice, num)) = lines.get() {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_bind_group(0, &self.main_bind_group, &[]);
            render_pass.set_bind_group(1, texture, &[]);
            render_pass.set_vertex_buffer(0, slice);
            render_pass.set_vertex_buffer(1, self.identity_instance_buffer.slice(..));
            render_pass.draw(0..num, 0..1);
        }
    }
}

fn create_render_pipeline(
    device: &wgpu::Device,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    label: &str,
    primitives: wgpu::PrimitiveTopology,
    vs_module: &wgpu::ShaderModule,
    fs_module: &wgpu::ShaderModule,
    alpha_blend: bool,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Cheese pipeline layout"),
        bind_group_layouts,
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
		label: Some(label),
		layout: Some(&pipeline_layout),
		vertex_stage: wgpu::ProgrammableStageDescriptor {
			module: vs_module,
			entry_point: "main",
		},
		fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
			module: fs_module,
			entry_point: "main",
		}),
		rasterization_state: Some(wgpu::RasterizationStateDescriptor {
			cull_mode: wgpu::CullMode::Back,
			..Default::default()
		}),
		primitive_topology: primitives,
		color_states: &[colour_state_descriptor(alpha_blend)],
		depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
			format: DEPTH_FORMAT,
			depth_write_enabled: true,
			depth_compare: wgpu::CompareFunction::Less,
			stencil: wgpu::StencilStateDescriptor::default(),
		}),
		vertex_state: wgpu::VertexStateDescriptor {
			index_format: wgpu::IndexFormat::Uint32,
			vertex_buffers: &[
				wgpu::VertexBufferDescriptor {
					stride: std::mem::size_of::<Vertex>() as u64,
					step_mode: wgpu::InputStepMode::Vertex,
					attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3, 2 => Float2],
				},
				wgpu::VertexBufferDescriptor {
					stride: std::mem::size_of::<ModelInstance>() as u64,
					step_mode: wgpu::InputStepMode::Instance,
					attributes: &wgpu::vertex_attr_array![3 => Float4, 4 => Float4, 5 => Float4, 6 => Float4, 7 => Float4],
				},
			],
		},
		sample_count: 1,
		sample_mask: !0,
		alpha_to_coverage_enabled: false,
	})
}

fn create_animated_pipeline(
    device: &wgpu::Device,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    vs_module: &wgpu::ShaderModule,
    fs_module: &wgpu::ShaderModule,
    alpha_blend: bool,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Cheese animated pipeline layout"),
        bind_group_layouts,
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
		label: Some("Cheese animated pipeline"),
		layout: Some(&pipeline_layout),
		vertex_stage: wgpu::ProgrammableStageDescriptor {
			module: vs_module,
			entry_point: "main",
		},
		fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
			module: fs_module,
			entry_point: "main",
		}),
		rasterization_state: Some(wgpu::RasterizationStateDescriptor {
			cull_mode: wgpu::CullMode::Back,
			..Default::default()
		}),
		primitive_topology: wgpu::PrimitiveTopology::TriangleList,
		color_states: &[colour_state_descriptor(alpha_blend)],
		depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
			format: DEPTH_FORMAT,
			depth_write_enabled: true,
			depth_compare: wgpu::CompareFunction::Less,
			stencil: wgpu::StencilStateDescriptor::default(),
		}),
		vertex_state: wgpu::VertexStateDescriptor {
			index_format: wgpu::IndexFormat::Uint32,
			vertex_buffers: &[
				wgpu::VertexBufferDescriptor {
					stride: std::mem::size_of::<AnimatedVertex>() as u64,
					step_mode: wgpu::InputStepMode::Vertex,
					attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3, 2 => Float2, 3 => Float4, 4 => Float4],
				},
				wgpu::VertexBufferDescriptor {
					stride: std::mem::size_of::<ModelInstance>() as u64,
					step_mode: wgpu::InputStepMode::Instance,
					attributes: &wgpu::vertex_attr_array![5 => Float4, 6 => Float4, 7 => Float4, 8 => Float4, 9 => Float4],
				},
			],
		},
		sample_count: 1,
		sample_mask: !0,
		alpha_to_coverage_enabled: false,
	})
}

fn colour_state_descriptor(alpha_blend: bool) -> wgpu::ColorStateDescriptor {
    if alpha_blend {
        wgpu::ColorStateDescriptor {
            format: DISPLAY_FORMAT,
            color_blend: wgpu::BlendDescriptor {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha_blend: wgpu::BlendDescriptor {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::DstAlpha,
                operation: wgpu::BlendOperation::Max,
            },
            write_mask: wgpu::ColorWrite::ALL,
        }
    } else {
        wgpu::ColorStateDescriptor {
            format: DISPLAY_FORMAT,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }
    }
}

pub struct ModelBuffers {
    pub mice: DynamicBuffer<ModelInstance>,
    pub mice_joints: DynamicBuffer<Mat4>,
    pub mice_joints_bind_group: wgpu::BindGroup,
    pub command_paths: DynamicBuffer<Vertex>,
    pub bullets: DynamicBuffer<ModelInstance>,
}

impl ModelBuffers {
    pub fn new(context: &RenderContext, assets: &Assets) -> Self {
        let mice_joints = DynamicBuffer::new(
            &context.device,
            400,
            "Cheese mice joints buffer",
            wgpu::BufferUsage::STORAGE,
        );

        Self {
            mice: DynamicBuffer::new(
                &context.device,
                50,
                "Cheese mice instance buffer",
                wgpu::BufferUsage::VERTEX,
            ),
            mice_joints_bind_group: create_joint_bind_group(
                context,
                "Cheese mice joints bind group",
                &mice_joints,
                &assets.mouse_model,
            ),
            mice_joints,
            bullets: DynamicBuffer::new(
                &context.device,
                200,
                "Cheese bullet buffer",
                wgpu::BufferUsage::VERTEX,
            ),
            command_paths: DynamicBuffer::new(
                &context.device,
                50,
                "Cheese command paths buffer",
                wgpu::BufferUsage::VERTEX,
            ),
        }
    }

    pub fn upload(&mut self, context: &RenderContext, assets: &Assets) {
        self.mice.upload(context);
        self.command_paths.upload(context);
        self.bullets.upload(context);
        let mice_resized = self.mice_joints.upload(context);

        // We need to recreate the bind group
        if mice_resized {
            self.mice_joints_bind_group = create_joint_bind_group(
                context,
                "Cheese mice joints bind group",
                &self.mice_joints,
                &assets.mouse_model,
            );
        }
    }
}

fn create_joint_bind_group(
    context: &RenderContext,
    label: &str,
    joint_buffer: &DynamicBuffer<Mat4>,
    model: &AnimatedModel,
) -> wgpu::BindGroup {
    context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &context.joint_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(joint_buffer.buffer.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(model.joint_uniforms.slice(..)),
                },
            ],
        })
}

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Debug)]
pub struct ModelInstance {
    pub flat_colour: Vec4,
    pub transform: Mat4,
}
