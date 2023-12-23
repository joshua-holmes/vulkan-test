use std::sync::Arc;

use vulkano::{
    VulkanLibrary,
    instance::{Instance, InstanceCreateInfo},
    device::{QueueFlags, Device, DeviceCreateInfo, QueueCreateInfo},
    memory::allocator::{StandardMemoryAllocator, AllocationCreateInfo, MemoryTypeFilter},
    buffer::{Buffer, BufferCreateInfo, BufferUsage, BufferContents},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder,
        CommandBufferUsage,
        CopyBufferInfo,
    },
    sync::{self, GpuFuture}
};

#[derive(BufferContents)]
#[repr(C)]
struct MyStruct {
    a: u32,
    b: u32,
}

fn main() {
    let library = VulkanLibrary::new().expect("no local Vulkan library/DLL");
    let instance = Instance::new(library, InstanceCreateInfo::default()).expect("failed to create instance");

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
            queue_family_properties.queue_flags.contains(QueueFlags::GRAPHICS)
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
        }
    )
    .expect("failed to create device");

    let queue = queues.next().unwrap();

    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));

    let source_content: Vec<i32> = (0..64).collect();
    let source = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        source_content,
    )
    .expect("failed to create source buffer");

    let destination_content: Vec<i32> = (0..64).map(|_| 0).collect();
    let destination = Buffer::from_iter(
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
        destination_content,
    )
    .expect("failed to create destination buffer");

    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        device.clone(),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );

    let mut builder = AutoCommandBufferBuilder::primary(
        &command_buffer_allocator,
        queue_family_index,
        CommandBufferUsage::OneTimeSubmit,
    )
    .expect("failed to create command buffer");

    builder.copy_buffer(
        CopyBufferInfo::buffers(source.clone(), destination.clone())
    ).unwrap();

    let command_buffer = builder.build().expect("failed to build command buffer");

    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .expect("could not execute order 66")
        .then_signal_fence_and_flush()
        .expect("failed to flush command buffer");
    
    future.wait(None).expect("failed to wait for the future");

    let future_src = source.read().expect("failed to read future source buffer");
    let future_dst = destination.read().expect("failed to read future destination buffer");
    assert_eq!(&*future_src, &*future_dst);

    println!("{:?}\n{:?}", &*future_src, &*future_dst);


}
