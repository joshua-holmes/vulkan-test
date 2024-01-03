use std::sync::Arc;

use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage},
    device::{Device, DeviceCreateInfo, QueueCreateInfo, QueueFlags},
    instance::{Instance, InstanceCreateInfo},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::graphics::vertex_input::Vertex as VertexMacro,
    VulkanLibrary,
};

#[derive(BufferContents, VertexMacro)]
#[repr(C)]
pub struct Vertex {
    #[format(R32G32B32_SFLOAT)]
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
}

mod shaders {
    vulkano_shaders::shader!{
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
