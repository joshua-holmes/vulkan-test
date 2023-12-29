use std::{sync::Arc, time::SystemTime};

use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferUsage,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, PersistentDescriptorSet, WriteDescriptorSet,
    },
    device::{Device, DeviceCreateInfo, QueueCreateInfo, QueueFlags},
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

mod cs;

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

    // setup original buffer
    let data_iter = 0..65536_u32;
    let data_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        data_iter.clone(),
    )
    .expect("failed to create data buffer");

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

    // setup descriptor
    let descriptor_set_allocator =
        StandardDescriptorSetAllocator::new(device.clone(), Default::default());
    let pipeline_layout = compute_pipeline.layout();
    let descriptor_set_layouts = pipeline_layout.set_layouts();
    let descriptor_set_layout_index = 0;
    let descriptor_set_layout = descriptor_set_layouts
        .get(descriptor_set_layout_index)
        .expect("could not get correct descriptor set");
    let descriptor_set = PersistentDescriptorSet::new(
        &descriptor_set_allocator,
        descriptor_set_layout.clone(),
        [WriteDescriptorSet::buffer(0, data_buffer.clone())],
        [],
    )
    .expect("failed to create descriptor set");

    // dispatch command buffer
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
    let work_group_counts = [1024, 1, 1];
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
        .expect("failed to dispatch work groups");
    let command_buffer = command_buffer_builder
        .build()
        .expect("failed to build command buffer");

    // submit command buffer
    let now_future = sync::now(device.clone());

    // time GPU's execution, for fun
    println!("Starting timer for GPU to compute...");
    let gpu_start = SystemTime::now();
    let future = now_future
        .then_execute(queue.clone(), command_buffer)
        .expect("failed to execute command buffer")
        .then_signal_fence_and_flush()
        .expect("failed to signal fence and flush");
    future.wait(None).unwrap();
    let gpu_elapsed = gpu_start.elapsed().expect("could not elapse gpu time");
    println!("Done\n");

    // time CPU's execution, for fun
    let mut cpu_buffer: Vec<_> = data_iter.collect();
    println!("Starting timer for CPU to compute...");
    let cpu_start = SystemTime::now();
    for n in cpu_buffer.iter_mut() {
        *n *= 12;
    }
    let cpu_elapsed = cpu_start.elapsed().expect("could not elapse cpu time");
    println!("Done\n");

    // check differences
    println!("GPU took this long: {:?}\nCPU took this long: {:?}\n", gpu_elapsed, cpu_elapsed);

    // check that exectution was correct
    println!("Checking that values match...");
    let content = data_buffer.read().expect("failed to read data buffer");
    for (gpu_val, cpu_val) in content.iter().zip(cpu_buffer.iter()) {
        assert_eq!(*gpu_val, *cpu_val);
    }
    println!("Values were equivelent");
}
