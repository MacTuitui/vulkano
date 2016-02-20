//! 
//! 
//! How does that work?
//! 
//! The shader analyser determines which descriptors are used by each shader and outputs a struct
//! that describes this stuff. This struct implements `PipelineLayoutDesc`.
//! 
//! When shaders are grouped together in the pipeline, they are passed through `CompatiblePipeline`
//! to be merged into a single pipeline layout struct. If the descriptors of the various shaders
//! are incompatible, it is detected at that moment.
//! 
//! The single struct that represents the layout is stored as a template parameter of the pipeline.
//! It is also stored in the `PipelineLayout` object (which wraps around vulkan's pipeline layout).
//! When you draw, you have to pass a collection of descriptor sets that are compatible with the
//! pipeline layout desc of the pipeline.
//! 
//! 

// FIXME: all destructors are missing


use std::mem;
use std::ptr;
use std::sync::Arc;

use buffer::BufferResource;
use device::Device;

use OomError;
use VulkanObject;
use VulkanPointers;
use check_errors;
use vk;

/// Types that describe the layout of a pipeline (descriptor sets and push constants).
pub unsafe trait PipelineLayoutDesc {
    type DescriptorSets;        // example: (Arc<DescriptorSet<Layout1>>, Arc<DescriptorSet<Layout2>>)   where Layout1 and Layout2 implement DescriptorSetDesc
    type DescriptorSetLayouts;      // example: (Arc<DescriptorSetLayout<Layout1>>, Arc<DescriptorSetLayout<Layout2>>)   where Layout1 and Layout2 implement DescriptorSetDesc
    type PushConstants;

    fn decode_descriptor_set_layouts(&self, Self::DescriptorSetLayouts) -> Vec<Arc<AbstractDescriptorSetLayout>>;

    fn decode_descriptor_sets(&self, Self::DescriptorSets) -> Vec<Arc<AbstractDescriptorSet>>;

    // FIXME: implement this correctly
    fn is_compatible_with<P>(&self, _: &P) -> bool where P: PipelineLayoutDesc { true }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct EmptyPipelineDesc;
unsafe impl PipelineLayoutDesc for EmptyPipelineDesc {
    type DescriptorSets = ();
    type DescriptorSetLayouts = ();
    type PushConstants = ();

    fn decode_descriptor_set_layouts(&self, _: Self::DescriptorSetLayouts) -> Vec<Arc<AbstractDescriptorSetLayout>> { vec![] }
    fn decode_descriptor_sets(&self, _: Self::DescriptorSets) -> Vec<Arc<AbstractDescriptorSet>> { vec![] }
}

/// Types that describe a single descriptor set.
pub unsafe trait DescriptorSetDesc {
    type Write;

    type Init;

    fn descriptors(&self) -> Vec<DescriptorDesc>;       // TODO: Cow for better perfs

    // FIXME: implement this correctly
    fn is_compatible_with<S>(&self, _: &S) -> bool where S: DescriptorSetDesc { true }

    fn decode_write(&self, Self::Write) -> Vec<DescriptorWrite>;

    fn decode_init(&self, Self::Init) -> Vec<DescriptorWrite>;
}

pub struct DescriptorDesc {
    pub binding: u32,
    pub ty: DescriptorType,
    pub count: u32,
    pub stages: ShaderStages,
}

// FIXME: add immutable sampler when relevant
#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum DescriptorType {
    Sampler = vk::DESCRIPTOR_TYPE_SAMPLER,
    CombinedImageSampler = vk::DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
    SampledImage = vk::DESCRIPTOR_TYPE_SAMPLED_IMAGE,
    StorageImage = vk::DESCRIPTOR_TYPE_STORAGE_IMAGE,
    UniformTexelBuffer = vk::DESCRIPTOR_TYPE_UNIFORM_TEXEL_BUFFER,
    StorageTexelBuffer = vk::DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER,
    UniformBuffer = vk::DESCRIPTOR_TYPE_UNIFORM_BUFFER,
    StorageBuffer = vk::DESCRIPTOR_TYPE_STORAGE_BUFFER,
    UniformBufferDynamic = vk::DESCRIPTOR_TYPE_UNIFORM_BUFFER_DYNAMIC,
    StorageBufferDynamic = vk::DESCRIPTOR_TYPE_STORAGE_BUFFER_DYNAMIC,
    InputAttachment = vk::DESCRIPTOR_TYPE_INPUT_ATTACHMENT,
}

impl DescriptorType {
    /// Turns the `DescriptorType` into the corresponding Vulkan constant.
    // this function exists because when immutable samplers are added, it will no longer be possible to do `as u32`
    #[inline]
    fn vk_enum(&self) -> u32 {
        *self as u32
    }
}

pub struct ShaderStages {
    pub vertex: bool,
    pub tessellation_control: bool,
    pub tessellation_evaluation: bool,
    pub geometry: bool,
    pub fragment: bool,
    pub compute: bool,
}

impl ShaderStages {
    /// Creates a `ShaderStages` struct will all graphics stages set to `true`.
    #[inline]
    pub fn all_graphics() -> ShaderStages {
        ShaderStages {
            vertex: true,
            tessellation_control: true,
            tessellation_evaluation: true,
            geometry: true,
            fragment: true,
            compute: false,
        }
    }

    /// Creates a `ShaderStages` struct will the compute stage set to `true`.
    #[inline]
    pub fn compute() -> ShaderStages {
        ShaderStages {
            vertex: false,
            tessellation_control: false,
            tessellation_evaluation: false,
            geometry: false,
            fragment: false,
            compute: true,
        }
    }
}

impl Into<vk::ShaderStageFlags> for ShaderStages {
    #[inline]
    fn into(self) -> vk::ShaderStageFlags {
        let mut result = 0;
        if self.vertex { result |= vk::SHADER_STAGE_VERTEX_BIT; }
        if self.tessellation_control { result |= vk::SHADER_STAGE_TESSELLATION_CONTROL_BIT; }
        if self.tessellation_evaluation { result |= vk::SHADER_STAGE_TESSELLATION_EVALUATION_BIT; }
        if self.geometry { result |= vk::SHADER_STAGE_GEOMETRY_BIT; }
        if self.fragment { result |= vk::SHADER_STAGE_FRAGMENT_BIT; }
        if self.compute { result |= vk::SHADER_STAGE_COMPUTE_BIT; }
        result
    }
}

/*
pub unsafe trait CompatiblePipeline<T> { type Out: PipelineLayoutDesc; }
pub unsafe trait CompatibleSet<T> { type Out: DescriptorSetDesc; }

macro_rules! impl_tuple {
    (($in_first:ident $out_first:ident) $(, ($in_rest:ident $out_rest:ident))*) => {
        unsafe impl<$in_first, $out_first $(, $in_rest, $out_rest)*>
            CompatibleSet<($out_first, $($out_rest,)*)> for ($in_first, $($in_rest,)*)
                where $in_first: CompatibleDescriptor<$out_first> $(, $in_rest: CompatibleDescriptor<$out_rest>)*
        {
            type Out = (
                <$in_first as CompatibleDescriptor<$out_first>>::Out,
                $(
                    <$in_rest as CompatibleDescriptor<$out_rest>>::Out,
                )*
            );
        }

        unsafe impl<$in_first, $out_first $(, $in_rest, $out_rest)*>
            CompatiblePipeline<($out_first, $($out_rest,)*)> for ($in_first, $($in_rest,)*)
                where $in_first: CompatibleSet<$out_first> $(, $in_rest: CompatibleSet<$out_rest>)*
        {
            type Out = (
                <$in_first as CompatibleSet<$out_first>>::Out,
                $(
                    <$in_rest as CompatibleSet<$out_rest>>::Out,
                )*
            );
        }

        impl_tuple!{$(($in_rest $out_rest)),*}
    };
    
    () => ();
}

impl_tuple!( (A N), (B O), (C P), (D Q), (E R), (F S), (G T),
             (H U), (I V), (J W), (K X), (L Y), (M Z) );

/// If a type `A` can be interpreted as a `T`, then `A` will implement `CompatibleDescriptor<T>`.
trait CompatibleDescriptor<T> { type Out; }

impl CompatibleDescriptor<()> for () { type Out = (); }
impl<T> CompatibleDescriptor<()> for T where T: Descriptor { type Out = T; }
impl<T> CompatibleDescriptor<T> for () where T: Descriptor { type Out = T; }
impl<T> CompatibleDescriptor<T> for T where T: Descriptor { type Out = T; }


pub unsafe trait Descriptor {}*/

pub struct DescriptorSet<S> {
    set: vk::DescriptorSet,
    pool: Arc<DescriptorPool>,
    layout: Arc<DescriptorSetLayout<S>>,
}

impl<S> DescriptorSet<S> where S: DescriptorSetDesc {
    ///
    /// # Panic
    ///
    /// - Panicks if the pool and the layout were not created from the same `Device`.
    ///
    pub fn new(pool: &Arc<DescriptorPool>, layout: &Arc<DescriptorSetLayout<S>>)
               -> Result<Arc<DescriptorSet<S>>, OomError>
    {
        assert_eq!(&*pool.device as *const Device, &*layout.device as *const Device);

        let vk = pool.device.pointers();

        let set = unsafe {
            let infos = vk::DescriptorSetAllocateInfo {
                sType: vk::STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
                pNext: ptr::null(),
                descriptorPool: pool.pool,
                descriptorSetCount: 1,
                pSetLayouts: &layout.layout,
            };

            let mut output = mem::uninitialized();
            try!(check_errors(vk.AllocateDescriptorSets(pool.device.internal_object(), &infos,
                                                        &mut output)));
            output
        };

        Ok(Arc::new(DescriptorSet {
            set: set,
            pool: pool.clone(),
            layout: layout.clone(),
        }))
    }

    pub fn write(&self, write: S::Write) {
        let vk = self.pool.device.pointers();
        let write = self.layout.description().decode_write(write);

        // FIXME: store resources in the descriptor set so that they aren't destroyed

        // TODO: the architecture of this function is going to be tricky

        let buffer_descriptors = write.iter().enumerate().map(|(num, write)| {
            match write.content {
                DescriptorBind::UniformBuffer(ref buffer) => {
                    Some(vk::DescriptorBufferInfo {
                        buffer: buffer.internal_object(),
                        offset: 0,      // FIXME: allow buffer slices
                        range: buffer.size() as u64,       // FIXME: allow buffer slices
                    })
                },
            }
        }).collect::<Vec<_>>();

        let vk_writes = write.iter().enumerate().map(|(num, write)| {
            vk::WriteDescriptorSet {
                sType: vk::STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: ptr::null(),
                dstSet: self.set,
                dstBinding: write.binding,
                dstArrayElement: write.array_element,
                descriptorCount: 1,
                descriptorType: vk::DESCRIPTOR_TYPE_UNIFORM_BUFFER,     // FIXME:
                pImageInfo: ptr::null(),        // FIXME:
                pBufferInfo: if let Some(ref b) = buffer_descriptors[num] { b } else { ptr::null() },
                pTexelBufferView: ptr::null(),      // FIXME:
            }
        }).collect::<Vec<_>>();

        unsafe {
            vk.UpdateDescriptorSets(self.pool.device.internal_object(), vk_writes.len() as u32,
                                    vk_writes.as_ptr(), 0, ptr::null());
        }
    }
}

impl<S> VulkanObject for DescriptorSet<S> {
    type Object = vk::DescriptorSet;

    #[inline]
    fn internal_object(&self) -> vk::DescriptorSet {
        self.set
    }
}

/// Trait that is implemented on all `DescriptorSet` objects.
pub unsafe trait AbstractDescriptorSet: ::VulkanObjectU64 {}
unsafe impl<S> AbstractDescriptorSet for DescriptorSet<S> {}

pub struct DescriptorSetLayout<S> {
    layout: vk::DescriptorSetLayout,
    device: Arc<Device>,
    description: S,
}

impl<S> DescriptorSetLayout<S> where S: DescriptorSetDesc {
    pub fn new(device: &Arc<Device>, description: S)
               -> Result<Arc<DescriptorSetLayout<S>>, OomError>
    {
        let vk = device.pointers();

        let bindings = description.descriptors().into_iter().map(|desc| {
            vk::DescriptorSetLayoutBinding {
                binding: desc.binding,
                descriptorType: desc.ty.vk_enum(),
                descriptorCount: desc.count,
                stageFlags: desc.stages.into(),
                pImmutableSamplers: ptr::null(),        // FIXME: not yet implemented
            }
        }).collect::<Vec<_>>();

        let layout = unsafe {
            let infos = vk::DescriptorSetLayoutCreateInfo {
                sType: vk::STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
                pNext: ptr::null(),
                flags: 0,   // reserved
                bindingCount: bindings.len() as u32,
                pBindings: bindings.as_ptr(),
            };

            let mut output = mem::uninitialized();
            try!(check_errors(vk.CreateDescriptorSetLayout(device.internal_object(), &infos,
                                                           ptr::null(), &mut output)));
            output
        };

        Ok(Arc::new(DescriptorSetLayout {
            layout: layout,
            device: device.clone(),
            description: description,
        }))
    }

    #[inline]
    pub fn description(&self) -> &S {
        &self.description
    }
}

impl<S> VulkanObject for DescriptorSetLayout<S> {
    type Object = vk::DescriptorSetLayout;

    #[inline]
    fn internal_object(&self) -> vk::DescriptorSetLayout {
        self.layout
    }
}

/// Trait that is implemented on all `DescriptorSetLayout` objects.
pub unsafe trait AbstractDescriptorSetLayout: ::VulkanObjectU64 {}
unsafe impl<S> AbstractDescriptorSetLayout for DescriptorSetLayout<S> {}

/// Represents the layout of all the resources and data that can be binded before drawing and
/// that will be accessible from the shaders.
///
/// The template parameter represents the descriptor sets.
// TODO: push constants.
pub struct PipelineLayout<P> {
    device: Arc<Device>,
    layout: vk::PipelineLayout,
    description: P,
    layouts: Vec<Arc<AbstractDescriptorSetLayout>>,     // TODO: is it necessary to keep the layouts alive? check the specs
}

impl<P> PipelineLayout<P> where P: PipelineLayoutDesc {
    /// Creates a new `PipelineLayout`.
    pub fn new(device: &Arc<Device>, description: P, layouts: P::DescriptorSetLayouts)
               -> Result<Arc<PipelineLayout<P>>, OomError>
    {
        let vk = device.pointers();

        let layouts = description.decode_descriptor_set_layouts(layouts);
        let layouts_ids = layouts.iter().map(|l| {
            // FIXME: check that they belong to the same device
            ::VulkanObjectU64::internal_object(&**l)
        }).collect::<Vec<_>>();

        let layout = unsafe {
            let infos = vk::PipelineLayoutCreateInfo {
                sType: vk::STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
                pNext: ptr::null(),
                flags: 0,   // reserved
                setLayoutCount: layouts_ids.len() as u32,
                pSetLayouts: layouts_ids.as_ptr(),
                pushConstantRangeCount: 0,      // TODO: unimplemented
                pPushConstantRanges: ptr::null(),    // TODO: unimplemented
            };

            let mut output = mem::uninitialized();
            try!(check_errors(vk.CreatePipelineLayout(device.internal_object(), &infos,
                                                      ptr::null(), &mut output)));
            output
        };

        Ok(Arc::new(PipelineLayout {
            device: device.clone(),
            layout: layout,
            description: description,
            layouts: layouts,
        }))
    }

    #[inline]
    pub fn description(&self) -> &P {
        &self.description
    }
}

impl<P> VulkanObject for PipelineLayout<P> {
    type Object = vk::PipelineLayout;

    #[inline]
    fn internal_object(&self) -> vk::PipelineLayout {
        self.layout
    }
}

pub struct DescriptorPool {
    pool: vk::DescriptorPool,
    device: Arc<Device>,
}

impl DescriptorPool {
    pub fn new(device: &Arc<Device>) -> Result<Arc<DescriptorPool>, OomError> {
        let vk = device.pointers();

        // FIXME: arbitrary
        let pool_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DESCRIPTOR_TYPE_UNIFORM_BUFFER,
                descriptorCount: 10,
            }
        ];

        let pool = unsafe {
            let infos = vk::DescriptorPoolCreateInfo {
                sType: vk::STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
                pNext: ptr::null(),
                flags: 0,   // TODO:
                maxSets: 100,       // TODO: let user choose
                poolSizeCount: pool_sizes.len() as u32,
                pPoolSizes: pool_sizes.as_ptr(),
            };

            let mut output = mem::uninitialized();
            try!(check_errors(vk.CreateDescriptorPool(device.internal_object(), &infos,
                                                      ptr::null(), &mut output)));
            output
        };

        Ok(Arc::new(DescriptorPool {
            pool: pool,
            device: device.clone(),
        }))
    }
}

// FIXME: shoud allow multiple array binds at once
pub struct DescriptorWrite {
    pub binding: u32,
    pub array_element: u32,
    pub content: DescriptorBind,
}

// FIXME: incomplete
#[derive(Clone)]        // TODO: Debug
pub enum DescriptorBind {
    UniformBuffer(Arc<BufferResource>),
}
