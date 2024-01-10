use std::sync::Arc;

use image::{ImageBuffer, Rgba};
use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferUsage, CopyImageToBufferInfo, RenderPassBeginInfo,
        SubpassBeginInfo, SubpassContents, SubpassEndInfo,
    },
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo,
        QueueFlags,
    },
    format::Format,
    image::{view::ImageView, Image, ImageCreateInfo, ImageType, ImageUsage},
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
    render_pass::{Framebuffer, FramebufferCreateInfo, Subpass},
    swapchain::{Surface, Swapchain, SwapchainCreateInfo},
    sync::{self, GpuFuture},
    VulkanLibrary,
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
    let (mut swapchain, images) = Swapchain::new(
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
    let my_triangle = Triangle::new([-0.5, -0.5], [0.5, -0.25], [0., 0.5]);

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
                format: Format::R8G8B8A8_UNORM,
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

    // create image
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [1024, 1024, 1],
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("could not create image");

    // create image view
    let view = ImageView::new_default(image.clone()).expect("failed to create image");
    let framebuffer = Framebuffer::new(
        render_pass.clone(),
        FramebufferCreateInfo {
            attachments: vec![view],
            ..Default::default()
        },
    )
    .expect("failed to create framebuffer");

    let buf = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        (0..1024 * 1024 * 4).map(|_| 0u8),
    )
    .expect("failed to create buffer to return image to host");
    // load shaders
    let vs = shaders::load_vertex(device.clone()).expect("failed to load vertex shader");
    let fs = shaders::load_fragment(device.clone()).expect("failed to load fragment shader");

    // setup viewport
    let viewport = Viewport {
        offset: [0.0, 0.0],
        extent: [1024.0, 1024.0],
        depth_range: 0.0..=1.0,
    };

    let pipeline = {
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
    };

    // create command buffer builder
    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        device.clone(),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );
    let mut builder = AutoCommandBufferBuilder::primary(
        &command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
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
        .draw(3, 1, 0, 0)
        .expect("failed to draw")
        .end_render_pass(SubpassEndInfo::default())
        .expect("cannot end render pass")
        .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, buf.clone()))
        .expect("failed to copy image to buffer");
    let command_buffer = builder.build().expect("failed to build command buffer");

    // execute
    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("failed to execute")
        .then_signal_fence_and_flush()
        .expect("failed to signal fench and flush");
    future.wait(None).unwrap();

    // get and save image
    let buffer_content = buf.read().expect("could not read buffer");
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(1024, 1024, buffer_content)
        .expect("could not create image from buffer");
    image.save("image.png").expect("failed to save image");

    // setup event loop
    event_loop.run(|event, _, control_flow| {
        control_flow.set_poll();

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("User requested window to be closed");
                control_flow.set_exit();
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
