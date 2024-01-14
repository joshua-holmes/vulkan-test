use std::sync::Arc;

use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo,
        SubpassBeginInfo, SubpassContents, SubpassEndInfo, PrimaryAutoCommandBuffer,
    },
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo,
        QueueFlags,
        Queue,
    },
    image::{view::ImageView, ImageUsage, Image},
    instance::{Instance, InstanceCreateInfo},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{
        graphics::{
            color_blend::{ColorBlendAttachmentState, ColorBlendState},
            input_assembly::InputAssemblyState,
            multisample::MultisampleState,
            rasterization::RasterizationState,
            vertex_input::{Vertex as VertexMacro, VertexDefinition},
            viewport::{Viewport, ViewportState},
            GraphicsPipelineCreateInfo,
        },
        layout::PipelineDescriptorSetLayoutCreateInfo,
        GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo,
    },
    render_pass::{Framebuffer, FramebufferCreateInfo, Subpass, RenderPass},
    swapchain::{Surface, Swapchain, SwapchainCreateInfo, self, SwapchainPresentInfo},
    sync::{self, GpuFuture, future::FenceSignalFuture},
    VulkanError,
    VulkanLibrary,
    shader::ShaderModule, Validated,
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

#[derive(BufferContents, VertexMacro)]
#[repr(C)]
pub struct Vertex {
    #[format(R32G32_SFLOAT)]
    pub position: [f32; 2],
}

pub struct Triangle(pub Vertex, pub Vertex, pub Vertex);

impl Triangle {
    fn new(point_1: [f32; 2], point_2: [f32; 2], point_3: [f32; 2]) -> Self {
        Self(
            Vertex { position: point_1 },
            Vertex { position: point_2 },
            Vertex { position: point_3 },
        )
    }

    fn move_verticies_out(self) -> Vec<Vertex> {
        vec![self.0, self.1, self.2]
    }
}

fn get_framebuffers(images: &Vec<Arc<Image>>, render_pass: Arc<RenderPass>) -> Vec<Arc<Framebuffer>> {
    images.iter().map(|i| {
        let view = ImageView::new_default(i.clone()).expect("failed to create image");
        Framebuffer::new(
            render_pass.clone(),
            FramebufferCreateInfo {
                attachments: vec![view],
                ..Default::default()
            },
        )
        .expect("failed to create framebuffer")
    })
    .collect()
}

fn get_pipeline(
    device: Arc<Device>,
    vs: Arc<ShaderModule>,
    fs: Arc<ShaderModule>,
    render_pass: Arc<RenderPass>,
    viewport: Viewport,
) -> Arc<GraphicsPipeline> {
    let vs = vs
        .entry_point("main")
        .expect("cannot find entry point for vertex shader");
    let fs = fs
        .entry_point("main")
        .expect("cannot find entry point for fragment shader");

    let vertext_input_state = Vertex::per_vertex()
        .definition(&vs.info().input_interface)
        .expect("could not build vertext input state for provided interface");

    let stages = [
        PipelineShaderStageCreateInfo::new(vs),
        PipelineShaderStageCreateInfo::new(fs),
    ];

    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone())
            .expect("failed to create pipeline descriptor set"),
    )
    .expect("failed to create pipeline layout");

    let subpass = Subpass::from(render_pass.clone(), 0).expect("could not create subpass");

    GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertext_input_state),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(ViewportState {
                viewports: [viewport].into_iter().collect(),
                ..Default::default()
            }),
            rasterization_state: Some(RasterizationState::default()),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default(),
            )),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("failed to create graphics pipeline")
}

fn get_command_buffers(
    command_buffer_allocator: &StandardCommandBufferAllocator,
    queue: &Arc<Queue>,
    pipeline: &Arc<GraphicsPipeline>,
    framebuffers: &Vec<Arc<Framebuffer>>,
    vertex_buffer: &Subbuffer<[Vertex]>,
) -> Vec<Arc<PrimaryAutoCommandBuffer>> {
    framebuffers.iter().map(|framebuffer| {
        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator,
            queue.queue_family_index(),
            CommandBufferUsage::MultipleSubmit,
        )
        .expect("could not build builder, ya know bob?");

        // build
        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some([0.0, 1.0, 0.0, 1.0].into())],
                    ..RenderPassBeginInfo::framebuffer(framebuffer.clone())
                },
                SubpassBeginInfo {
                    contents: SubpassContents::Inline,
                    ..Default::default()
                },
            )
            .expect("cannot begin render pass")
            .bind_pipeline_graphics(pipeline.clone())
            .expect("cannot bind pipeline")
            .bind_vertex_buffers(0, vertex_buffer.clone())
            .expect("cannot bind vertex buffer")
            .draw(vertex_buffer.len() as u32, 1, 0, 0)
            .expect("failed to draw")
            .end_render_pass(SubpassEndInfo::default())
            .expect("cannot end render pass");

        builder.build().unwrap()
    })
    .collect()
}

fn main() {
    // setup vulkan and window
    let event_loop = EventLoop::new();
    let library = VulkanLibrary::new().expect("no local Vulkan library/DLL");
    let required_extensions = Surface::required_extensions(&event_loop);
    let window = Arc::new(Window::new(&event_loop).expect("failed to create window"));
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: required_extensions,
            ..Default::default()
        },
    )
    .expect("failed to create instance");
    let surface = Surface::from_window(instance.clone(), window.clone())
        .expect("failed to create surface from window");

    // setup device
    let device_extensions = DeviceExtensions {
        khr_swapchain: true,
        ..Default::default()
    };
    let (physical_device, queue_family_index) = instance
        .enumerate_physical_devices()
        .expect("could not enumerate devices")
        .filter(|p| p.supported_extensions().contains(&device_extensions))
        .filter_map(|p| {
            p.queue_family_properties()
                .iter()
                .enumerate()
                .position(|(i, q)| {
                    q.queue_flags.contains(QueueFlags::GRAPHICS)
                        && p.surface_support(i as u32, &surface).unwrap_or(false)
                })
                .map(|q| (p, q as u32))
        })
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            _ => 4,
        })
        .expect("no device available");
    let (device, mut queues) = Device::new(
        physical_device.clone(),
        DeviceCreateInfo {
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            enabled_extensions: device_extensions,
            ..Default::default()
        },
    )
    .expect("failed to create device");
    let queue = queues.next().unwrap();
    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));

    // create swapchain
    let capabilities = physical_device
        .surface_capabilities(&surface, Default::default())
        .expect("failed to get surface capabilities");
    let dimensions = window.inner_size();
    let composite_alpha = capabilities.supported_composite_alpha.into_iter().next().unwrap();
    let image_format = physical_device
        .surface_formats(&surface, Default::default())
        .unwrap()[0]
        .0;
    let (mut my_swapchain, images) = Swapchain::new(
        device.clone(),
        surface.clone(),
        SwapchainCreateInfo {
            min_image_count: capabilities.min_image_count + 1,
            image_format,
            image_extent: dimensions.into(),
            image_usage: ImageUsage::COLOR_ATTACHMENT,
            composite_alpha,
            ..Default::default()
        }
    )
    .expect("failed to create swapchain");

    // setup a triangle
    let my_triangle = Triangle::new([-0.5, 0.5], [0.8, 0.1], [-0.2, -0.6]);

    // setup buffer
    let vertex_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        my_triangle.move_verticies_out(),
    )
    .expect("failed to create vertex buffer");

    // setup render pass
    let render_pass = vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            clear_color: {
                format: my_swapchain.image_format(),
                samples: 1,
                load_op: Clear,
                store_op: Store,
            },
        },
        pass: {
            color: [clear_color],
            depth_stencil: {}
        },
    )
    .expect("failed to instantiate render pass");

    // create image view
    let framebuffers = get_framebuffers(&images, render_pass.clone());

    // load shaders
    let vs = shaders::load_vertex(device.clone()).expect("failed to load vertex shader");
    let fs = shaders::load_fragment(device.clone()).expect("failed to load fragment shader");

    // setup viewport
    let mut viewport = Viewport {
        offset: [0.0, 0.0],
        extent: [1024.0, 1024.0],
        depth_range: 0.0..=1.0,
    };

    let pipeline = get_pipeline(
        device.clone(), 
        vs.clone(), 
        fs.clone(),
        render_pass.clone(),
        viewport.clone()
    );

    // create command buffers
    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        device.clone(),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );
    let mut command_buffers = get_command_buffers(
        &command_buffer_allocator,
        &queue,
        &pipeline,
        &framebuffers,
        &vertex_buffer
    );

    // setup fences vector so CPU doesn't have to wait for GPU
    let frames_in_flight = images.len();
    let mut fences: Vec<Option<Arc<FenceSignalFuture<_>>>> = vec![None; frames_in_flight];
    let mut previous_fence_i = 0;
    let mut recreate_swapchain = false;
    let mut window_resized = false;

    // setup event loop
    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("User requested window to be closed");
                control_flow.set_exit();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                window_resized = true;
            }
            Event::MainEventsCleared => {
                if recreate_swapchain || window_resized {
                    recreate_swapchain = false;

                    let new_dimensions = window.inner_size();

                    let (new_swapchain, new_images) = my_swapchain
                        .recreate(SwapchainCreateInfo {
                            image_extent: new_dimensions.into(),
                            ..my_swapchain.create_info()
                        })
                        .expect("failed to recreate swapchain");
                    my_swapchain = new_swapchain;

                    if window_resized {
                        window_resized = false;

                        let new_framebuffers = get_framebuffers(&new_images, render_pass.clone());

                        viewport.extent = new_dimensions.into();

                        let new_pipeline = get_pipeline(
                            device.clone(), 
                            vs.clone(), 
                            fs.clone(),
                            render_pass.clone(),
                            viewport.clone()
                        );

                        command_buffers = get_command_buffers(
                            &command_buffer_allocator,
                            &queue,
                            &new_pipeline,
                            &new_framebuffers,
                            &vertex_buffer,
                        );
                    }
                }

                let (image_i, suboptimal, acquire_future) = 
                    match swapchain::acquire_next_image(my_swapchain.clone(), None) {
                        Ok(r) => r,
                        Err(e) => panic!("failed to acquire next image: {}", e),
                    };

                if suboptimal {
                    recreate_swapchain = true;
                    println!("WARNING: swapchain function is suboptimal");
                }

                if let Some(image_fence) = &fences[image_i as usize] {
                    image_fence.wait(None).unwrap();
                }

                let previous_future = match fences[previous_fence_i as usize].clone() {
                    None => {
                        let mut now = sync::now(device.clone());
                        now.cleanup_finished();
                        now.boxed()
                    }
                    Some(fence) => fence.boxed(),
                };

                let future = previous_future
                    .join(acquire_future)
                    .then_execute(queue.clone(), command_buffers[image_i as usize].clone())
                    .expect("failed to execute command buffer")
                    .then_swapchain_present(
                        queue.clone(),
                        SwapchainPresentInfo::swapchain_image_index(my_swapchain.clone(), image_i)
                    )
                    .then_signal_fence_and_flush();

                fences[image_i as usize] = match future.map_err(Validated::unwrap) {
                    Ok(value) => Some(Arc::new(value)),
                    Err(VulkanError::OutOfDate) => {
                        recreate_swapchain = true;
                        None
                    }
                    Err(e) => {
                        println!("failed to flush future from img '{}': {}", image_i, e);
                        None
                    }
                };

                previous_fence_i = image_i;
            }
            _ => (),
        }
    });
}

mod shaders {
    vulkano_shaders::shader! {
        shaders: {
            vertex: {
                ty: "vertex",
                path: "examples/graphics/shader.vert"
            },
            fragment: {
                ty: "fragment",
                path: "examples/graphics/shader.frag"
            },
        }
    }
}
