//! Typed resource ports and Host-issued immutable native outputs.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    WgpuRenderProducerId,
    graph::{
        AccessRole, BufferMeaning, Initialization, ResourceDescriptor, ResourceRead,
        ResourceVersion, ResourceWrite, TextureAlpha, TextureMeaning, TextureTransfer,
    },
    output::OutputReservation,
};

use super::WgpuRenderProgramError;

/// Exact whole-texture shape and interpretation. The semantic port is its Rust marker type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuProgramTextureDescriptor {
    pub native: wgpu::TextureDescriptor<'static>,
    pub transfer: TextureTransfer,
    pub alpha: TextureAlpha,
}

impl WgpuProgramTextureDescriptor {
    pub(crate) fn resource<S: 'static>(&self) -> ResourceDescriptor {
        ResourceDescriptor::Texture {
            native: self.native.clone(),
            range: wgpu::ImageSubresourceRange::default(),
            meaning: TextureMeaning::of::<S>(self.transfer, self.alpha),
        }
    }
}

/// One candidate's typed texture version. Cloning does not create another writer or GPU texture.
#[derive(Debug)]
pub struct WgpuProgramTexture<S: 'static> {
    pub(crate) version: ResourceVersion,
    pub(super) descriptor: WgpuProgramTextureDescriptor,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuProgramTexture<S> {
    fn clone(&self) -> Self {
        Self::new(self.version.clone(), self.descriptor.clone())
    }
}

impl<S: 'static> WgpuProgramTexture<S> {
    pub(super) fn new(version: ResourceVersion, descriptor: WgpuProgramTextureDescriptor) -> Self {
        Self {
            version,
            descriptor,
            semantic: PhantomData,
        }
    }

    #[must_use]
    pub fn descriptor(&self) -> &WgpuProgramTextureDescriptor {
        &self.descriptor
    }

    /// Declares a typed read. Native access is available only inside this operation's scope.
    #[must_use]
    pub fn read(&self, role: AccessRole) -> WgpuProgramRead {
        WgpuProgramRead(ResourceRead {
            version: self.version.clone(),
            role,
            expected: self.descriptor.resource::<S>(),
        })
    }

    /// Clear the whole output before the adapter executes, independently of its viewport.
    #[must_use]
    pub fn clear(&self, color: wgpu::Color, role: AccessRole) -> WgpuProgramWrite {
        WgpuProgramWrite(ResourceWrite {
            version: self.version.clone(),
            role,
            initialization: Initialization::Clear(color),
        })
    }

    /// Copy an exact physical predecessor into distinct storage, then execute the adapter.
    /// Different semantic marker types are allowed for this explicit initialization copy.
    #[must_use]
    pub fn preserve<P: 'static>(
        &self,
        predecessor: &WgpuProgramTexture<P>,
        role: AccessRole,
    ) -> WgpuProgramWrite {
        WgpuProgramWrite(ResourceWrite {
            version: self.version.clone(),
            role,
            initialization: Initialization::Preserve(predecessor.version.clone()),
        })
    }

    #[must_use]
    pub fn output(&self) -> WgpuProgramOutput {
        WgpuProgramOutput(self.version.clone())
    }
}

/// One candidate's typed whole-buffer version, independent of its native usage flags.
#[derive(Debug)]
pub struct WgpuProgramBuffer<S: 'static> {
    pub(crate) version: ResourceVersion,
    pub(super) descriptor: wgpu::BufferDescriptor<'static>,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuProgramBuffer<S> {
    fn clone(&self) -> Self {
        Self::new(self.version.clone(), self.descriptor.clone())
    }
}

impl<S: 'static> WgpuProgramBuffer<S> {
    pub(super) fn new(
        version: ResourceVersion,
        descriptor: wgpu::BufferDescriptor<'static>,
    ) -> Self {
        Self {
            version,
            descriptor,
            semantic: PhantomData,
        }
    }

    #[must_use]
    pub fn descriptor(&self) -> &wgpu::BufferDescriptor<'static> {
        &self.descriptor
    }

    pub(crate) fn resource(&self) -> ResourceDescriptor {
        ResourceDescriptor::Buffer {
            native: self.descriptor.clone(),
            meaning: BufferMeaning::of::<S>(),
        }
    }

    #[must_use]
    pub fn read(&self, role: AccessRole) -> WgpuProgramRead {
        WgpuProgramRead(ResourceRead {
            version: self.version.clone(),
            role,
            expected: self.resource(),
        })
    }

    /// Zero the whole output before the adapter executes, independent of allocator contents.
    #[must_use]
    pub fn zero(&self, role: AccessRole) -> WgpuProgramWrite {
        WgpuProgramWrite(ResourceWrite {
            version: self.version.clone(),
            role,
            initialization: Initialization::Zero,
        })
    }

    #[must_use]
    pub fn preserve<P: 'static>(
        &self,
        predecessor: &WgpuProgramBuffer<P>,
        role: AccessRole,
    ) -> WgpuProgramWrite {
        WgpuProgramWrite(ResourceWrite {
            version: self.version.clone(),
            role,
            initialization: Initialization::Preserve(predecessor.version.clone()),
        })
    }

    #[must_use]
    pub fn output(&self) -> WgpuProgramOutput {
        WgpuProgramOutput(self.version.clone())
    }
}

/// One declared read of a candidate-local resource. Construct through a typed port.
#[derive(Clone, Debug)]
pub struct WgpuProgramRead(pub(super) ResourceRead);

/// One declared writer and its explicit initialization. Construct through a typed port.
#[derive(Clone, Debug)]
pub struct WgpuProgramWrite(pub(super) ResourceWrite);

/// An exact resource selected for the candidate's complete publication group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuProgramOutput(pub(super) ResourceVersion);

#[derive(Debug)]
pub(crate) enum NativeResource {
    Texture {
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    },
    Buffer(wgpu::Buffer),
}

/// The physical allocation owner is shared by published outputs and every submitted use.
#[derive(Debug)]
pub(crate) struct ProgramResourceStorage {
    pub(crate) backend_instance_id: u64,
    pub(crate) device_epoch: u64,
    pub(crate) epoch_lost: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) producer: WgpuRenderProducerId,
    pub(crate) source: nara_render::RenderFrameSourceGeneration,
    pub(crate) frame_index: u64,
    pub(crate) descriptor: ResourceDescriptor,
    pub(crate) native: NativeResource,
    pub(crate) _reservation: Arc<OutputReservation>,
}

/// Host-owned storage for one graph-local final texture, initialized by its normal writer.
/// Preallocation supplies physical custody only; it does not authorize an initialized import.
pub(crate) struct HostProgramTexture {
    pub(crate) version: ResourceVersion,
    pub(crate) storage: Arc<ProgramResourceStorage>,
}

impl ProgramResourceStorage {
    pub(crate) fn is_current(&self) -> bool {
        !self.epoch_lost.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Exact initialized texture pixels issued after successful Host submission admission.
///
/// This lease grants read-only import, not producer interaction, submission or reconstruction.
pub struct WgpuProgramTextureOutput<S: 'static> {
    pub(crate) storage: Arc<ProgramResourceStorage>,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuProgramTextureOutput<S> {
    fn clone(&self) -> Self {
        Self::new(Arc::clone(&self.storage))
    }
}

impl<S: 'static> std::fmt::Debug for WgpuProgramTextureOutput<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuProgramTextureOutput")
            .field("storage", &self.storage)
            .finish()
    }
}

impl<S: 'static> WgpuProgramTextureOutput<S> {
    pub(crate) fn new(storage: Arc<ProgramResourceStorage>) -> Self {
        Self {
            storage,
            semantic: PhantomData,
        }
    }

    #[must_use]
    pub fn is_current(&self) -> bool {
        self.storage.is_current()
    }

    #[must_use]
    pub fn descriptor(&self) -> WgpuProgramTextureDescriptor {
        let ResourceDescriptor::Texture {
            native, meaning, ..
        } = &self.storage.descriptor
        else {
            unreachable!("typed texture outputs are issued only for texture storage")
        };
        WgpuProgramTextureDescriptor {
            native: native.clone(),
            transfer: meaning.transfer,
            alpha: meaning.alpha,
        }
    }

    #[must_use]
    pub fn producer_id(&self) -> WgpuRenderProducerId {
        self.storage.producer
    }

    #[must_use]
    pub fn source_generation(&self) -> nara_render::RenderFrameSourceGeneration {
        self.storage.source
    }

    #[must_use]
    pub fn frame_index(&self) -> u64 {
        self.storage.frame_index
    }
}

/// Exact initialized buffer bytes issued after successful Host submission admission.
pub struct WgpuProgramBufferOutput<S: 'static> {
    pub(crate) storage: Arc<ProgramResourceStorage>,
    semantic: PhantomData<fn() -> S>,
}

impl<S: 'static> Clone for WgpuProgramBufferOutput<S> {
    fn clone(&self) -> Self {
        Self::new(Arc::clone(&self.storage))
    }
}

impl<S: 'static> std::fmt::Debug for WgpuProgramBufferOutput<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuProgramBufferOutput")
            .field("storage", &self.storage)
            .finish()
    }
}

impl<S: 'static> WgpuProgramBufferOutput<S> {
    pub(crate) fn new(storage: Arc<ProgramResourceStorage>) -> Self {
        Self {
            storage,
            semantic: PhantomData,
        }
    }

    #[must_use]
    pub fn is_current(&self) -> bool {
        self.storage.is_current()
    }

    #[must_use]
    pub fn descriptor(&self) -> &wgpu::BufferDescriptor<'static> {
        let ResourceDescriptor::Buffer { native, .. } = &self.storage.descriptor else {
            unreachable!("typed buffer outputs are issued only for buffer storage")
        };
        native
    }

    #[must_use]
    pub fn producer_id(&self) -> WgpuRenderProducerId {
        self.storage.producer
    }

    #[must_use]
    pub fn source_generation(&self) -> nara_render::RenderFrameSourceGeneration {
        self.storage.source
    }

    #[must_use]
    pub fn frame_index(&self) -> u64 {
        self.storage.frame_index
    }
}

pub(crate) fn require_descriptor<S: 'static>(
    actual: &ResourceDescriptor,
    texture: bool,
) -> Result<(), WgpuRenderProgramError> {
    let semantic = match actual {
        ResourceDescriptor::Texture { meaning, .. } if texture => meaning.semantic,
        ResourceDescriptor::Buffer { meaning, .. } if !texture => meaning.semantic,
        _ => return Err(WgpuRenderProgramError::ResourceKind),
    };
    if semantic != std::any::TypeId::of::<S>() {
        return Err(WgpuRenderProgramError::ResourceSemantic);
    }
    Ok(())
}
