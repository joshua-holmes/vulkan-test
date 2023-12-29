use std::sync::Arc;

use image::{ImageBuffer, Rgba};
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, CopyBufferToImageInfo,
    },
    device::{Device, DeviceCreateInfo, QueueCreateInfo, QueueFlags},
    format::{ClearColorValue, Format},
    image::{Image, ImageCreateInfo, ImageType, ImageUsage},
    instance::{Instance, InstanceCreateInfo},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    VulkanLibrary, sync::{self, GpuFuture},
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

    // create image
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [1024, 1024, 1],
            usage: ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .expect("could not create image");
    let pixel_data_iter = (0..1024 * 1024 * 4).enumerate().map(|(i, _)| {
        match i % 4 {
            0 => 255, // red
            1 => 0,   // green
            2 => 0,   // blue
            3 => 255, // alpha
            _ => unreachable!("`i % 4` should only contain numbers 0-3 (inclusive)")
        }
    });

    // create buffer from image
    let buf = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST | BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        pixel_data_iter,
    )
    .expect("failed to create buffer");

    // dispatch command buffer
    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        device.clone(),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );
    let mut builder = AutoCommandBufferBuilder::primary(
        &command_buffer_allocator,
        queue_family_index,
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("failed to create command buffer builder");
    builder
        .clear_color_image(ClearColorImageInfo {
            clear_value: ClearColorValue::Float([0.0, 0.0, 1.0, 1.0]),
            ..ClearColorImageInfo::image(image.clone())
        })
        .expect("could not clear image")
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            buf.clone(),
            image.clone(),
        ))
        .expect("failed to copy buffer to image");
    let command_buffer = builder.build().expect("failed to build command buffer");

    // execute
    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("failed to execute command buffer")
        .then_signal_fence_and_flush()
        .unwrap();
    future.wait(None).unwrap();

    // extract image
    let buffer_content = buf.read().expect("failed to read buffer");
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(1024, 1024, buffer_content).expect("failed to extract image");
    image.save("image.png").expect("could not save image");
}
