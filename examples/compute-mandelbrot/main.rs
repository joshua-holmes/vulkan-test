use std::{sync::Arc, time::SystemTime};

use image::{ImageBuffer, Rgba};
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferUsage, CopyImageToBufferInfo,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, PersistentDescriptorSet, WriteDescriptorSet,
    },
    device::{Device, DeviceCreateInfo, QueueCreateInfo, QueueFlags},
    format::Format,
    image::{view::ImageView, Image, ImageCreateInfo, ImageType, ImageUsage},
    instance::{Instance, InstanceCreateInfo},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{
        compute::ComputePipelineCreateInfo, layout::PipelineDescriptorSetLayoutCreateInfo,
        ComputePipeline, Pipeline, PipelineBindPoint, PipelineLayout,
        PipelineShaderStageCreateInfo,
    },
    sync::{self, GpuFuture},
    VulkanLibrary,
};

fn main() {
    // setup vulkan
    let library = VulkanLibrary::new().expect("no local Vulkan library/DLL");
    let instance =
        Instance::new(library, InstanceCreateInfo::default()).expect("failed to create instance");

    // setup device
    let physical_device = instance
        .enumerate_physical_devices()
        .expect("could not enumerate devices")
        .next()
        .expect("no devices available");
    let queue_family_index = physical_device
        .queue_family_properties()
        .iter()
        .enumerate()
        .position(|(_i, queue_family_properties)| {
            queue_family_properties
                .queue_flags
                .contains(QueueFlags::GRAPHICS)
        })
        .expect("couldn't find a graphical queue fmaily") as u32;
    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .expect("failed to create device");
    let queue = queues.next().unwrap();
    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));

    // setup compute pipeline
    let shader = cs::load(device.clone()).expect("failed to create shader module");
    let entry_point = shader
        .entry_point("main")
        .expect("failed to create entry point");
    let stage = PipelineShaderStageCreateInfo::new(entry_point);
    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
            .into_pipeline_layout_create_info(device.clone())
            .expect("could not create pipeline layout info"),
    )
    .expect("could not create pipeline layout");
    let compute_pipeline = ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(stage, layout),
    )
    .expect("failed to create compute pipeline");

    // setup image input
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [1024, 1024, 1],
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("failed to create image");
    let image_view = ImageView::new_default(image.clone()).expect("could not create image view");

    // setup descriptor
    let descriptor_set_allocator =
        StandardDescriptorSetAllocator::new(device.clone(), Default::default());
    let descriptor_set_layout_index = 0;
    let descriptor_set_layout = compute_pipeline
        .layout()
        .set_layouts()
        .get(descriptor_set_layout_index)
        .expect("could not get correct descriptor set");
    let descriptor_set = PersistentDescriptorSet::new(
        &descriptor_set_allocator,
        descriptor_set_layout.clone(),
        [WriteDescriptorSet::image_view(
            descriptor_set_layout_index as u32,
            image_view.clone(),
        )],
        [],
    )
    .expect("failed to create descriptor set");

    // create buffer for image output
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
    .expect("could not create buffer");

    // create buffer builder
    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        device.clone(),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        &command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("failed to create command buffer builder");

    // build buffer
    let work_group_counts = [1024 / 8, 1024 / 8, 1];
    command_buffer_builder
        .bind_pipeline_compute(compute_pipeline.clone())
        .expect("failed to bind pipeline command buffer builder")
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            compute_pipeline.layout().clone(),
            descriptor_set_layout_index as u32,
            descriptor_set,
        )
        .expect("failed to bind command buffer to descriptor sets")
        .dispatch(work_group_counts)
        .expect("failed to dispatch work groups")
        .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(
            image.clone(),
            buf.clone(),
        ))
        .unwrap();
    let command_buffer = command_buffer_builder
        .build()
        .expect("failed to build command buffer");

    // submit command buffer
    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("failed to execute")
        .then_signal_fence_and_flush()
        .unwrap();

    future.wait(None).unwrap();
    
    // read buffer
    let buf_content = buf.read().expect("could not read buffer");
    let image_buf = ImageBuffer::<Rgba<u8>, _>::from_raw(1024, 1024, &buf_content[..]).expect("failed to create image from buffer");
    image_buf.save("mandelbrot.png").expect("failed to save image");
}

mod cs {
    vulkano_shaders::shader! {
        ty: "compute",
        path: "examples/compute-mandelbrot/shader.glsl"
    }
}
