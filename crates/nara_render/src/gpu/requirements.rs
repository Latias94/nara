//! Exact native prerequisites retained by immutable render Feature definitions.

use thiserror::Error;

/// Required capabilities of the stock WGPU device, declared before its acquisition.
///
/// This is an owned declaration, not an adapter-support or enabled-device snapshot. Every
/// declaration includes WGPU's default device limits, matching the stock renderer's baseline
/// and WGPU's normalization of device requests. Only explicitly declared features are requested.
/// Definitions can share or clone this value without acquiring native device authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WgpuDeviceRequirements {
    required_features: wgpu::Features,
    required_limits: wgpu::Limits,
}

impl WgpuDeviceRequirements {
    #[must_use]
    pub fn new(required_features: wgpu::Features, required_limits: wgpu::Limits) -> Self {
        Self {
            required_features,
            required_limits: wgpu::Limits::default().or_better_values_from(&required_limits),
        }
    }

    #[must_use]
    pub const fn required_features(&self) -> wgpu::Features {
        self.required_features
    }

    #[must_use]
    pub const fn required_limits(&self) -> &wgpu::Limits {
        &self.required_limits
    }

    /// Combines the exact feature union and the strongest requirement for each WGPU limit.
    ///
    /// WGPU owns the direction of every limit: maximum capacities grow, while minimum buffer
    /// offset alignments shrink. Adapter support never broadens this declaration.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            required_features: self.required_features | other.required_features,
            required_limits: self
                .required_limits
                .clone()
                .or_better_values_from(&other.required_limits),
        }
    }

    pub(crate) fn validate_request(&self) -> Result<(), WgpuDeviceRequirementsError> {
        let experimental = self.required_features & wgpu::Features::all_experimental_mask();
        if !experimental.is_empty() {
            return Err(
                WgpuDeviceRequirementsError::ExperimentalFeaturesUnsupported {
                    features: experimental,
                },
            );
        }
        Ok(())
    }

    pub(crate) fn validate_adapter(
        &self,
        supported_features: wgpu::Features,
        supported_limits: &wgpu::Limits,
    ) -> Result<(), WgpuDeviceRequirementsError> {
        self.validate_request()?;
        let missing = self.required_features - supported_features;
        if !missing.is_empty() {
            return Err(WgpuDeviceRequirementsError::UnsupportedFeatures { missing });
        }
        let mut failure = Ok(());
        self.required_limits.check_limits_with_fail_fn(
            supported_limits,
            true,
            |name, required, supported| {
                failure = Err(WgpuDeviceRequirementsError::UnsupportedLimit {
                    name,
                    required,
                    supported,
                });
            },
        );
        failure
    }

    pub(crate) fn validate_enabled(
        &self,
        enabled_features: wgpu::Features,
        enabled_limits: &wgpu::Limits,
    ) -> Result<(), WgpuDeviceRequirementsError> {
        let missing = self.required_features - enabled_features;
        if !missing.is_empty() {
            return Err(WgpuDeviceRequirementsError::RequiredFeaturesNotEnabled { missing });
        }
        let mut failure = Ok(());
        self.required_limits.check_limits_with_fail_fn(
            enabled_limits,
            true,
            |name, required, enabled| {
                failure = Err(WgpuDeviceRequirementsError::RequiredLimitNotEnabled {
                    name,
                    required,
                    enabled,
                });
            },
        );
        failure
    }

    pub(crate) fn is_satisfied_by(&self, features: wgpu::Features, limits: &wgpu::Limits) -> bool {
        features.contains(self.required_features) && self.required_limits.check_limits(limits)
    }

    pub(crate) fn validate_late_request(
        &self,
        supported_features: wgpu::Features,
        supported_limits: &wgpu::Limits,
        enabled_features: wgpu::Features,
        enabled_limits: &wgpu::Limits,
    ) -> Result<(), WgpuDeviceRequirementsError> {
        self.validate_adapter(supported_features, supported_limits)?;
        if !self.is_satisfied_by(enabled_features, enabled_limits) {
            return Err(WgpuDeviceRequirementsError::ReconstructionRequired);
        }
        Ok(())
    }

    pub(crate) fn descriptor(&self) -> wgpu::DeviceDescriptor<'static> {
        wgpu::DeviceDescriptor {
            label: Some("nara-render-device"),
            required_features: self.required_features,
            required_limits: self.required_limits.clone(),
            ..Default::default()
        }
    }
}

/// A prerequisite refusal, distinct from adapter/device acquisition failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WgpuDeviceRequirementsError {
    #[error("the selected wgpu adapter does not support required features {missing:?}")]
    UnsupportedFeatures { missing: wgpu::Features },
    #[error("the selected wgpu adapter limit {name} supports {supported}, required {required}")]
    UnsupportedLimit {
        name: &'static str,
        required: u64,
        supported: u64,
    },
    #[error("the acquired wgpu device did not enable requested features {missing:?}")]
    RequiredFeaturesNotEnabled { missing: wgpu::Features },
    #[error("the acquired wgpu device limit {name} enabled {enabled}, requested {required}")]
    RequiredLimitNotEnabled {
        name: &'static str,
        required: u64,
        enabled: u64,
    },
    #[error("the stock render device does not admit experimental wgpu features {features:?}")]
    ExperimentalFeaturesUnsupported { features: wgpu::Features },
    #[error("render device requirements need explicit Host reconstruction")]
    ReconstructionRequired,
    #[error("the render device owner is unavailable and must be reconstructed")]
    BackendUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_uses_wgpu_limit_directions_and_only_declared_features() {
        let first = WgpuDeviceRequirements::new(
            wgpu::Features::TIMESTAMP_QUERY,
            wgpu::Limits {
                max_buffer_size: 512 * 1024 * 1024,
                max_compute_workgroup_storage_size: 32768,
                min_uniform_buffer_offset_alignment: 64,
                min_storage_buffer_offset_alignment: 128,
                ..Default::default()
            },
        );
        let second = WgpuDeviceRequirements::new(
            wgpu::Features::TEXTURE_COMPRESSION_BC,
            wgpu::Limits {
                max_buffer_size: 1024 * 1024 * 1024,
                min_uniform_buffer_offset_alignment: 128,
                min_storage_buffer_offset_alignment: 64,
                ..Default::default()
            },
        );
        let combined = first.union(&second);
        assert_eq!(combined, second.union(&first));
        assert_eq!(combined, combined.union(&combined));
        assert_eq!(
            combined.required_features(),
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TEXTURE_COMPRESSION_BC
        );
        assert_eq!(
            combined.required_limits().max_buffer_size,
            1024 * 1024 * 1024
        );
        assert_eq!(
            combined
                .required_limits()
                .max_compute_workgroup_storage_size,
            32768
        );
        assert_eq!(
            combined
                .required_limits()
                .min_uniform_buffer_offset_alignment,
            64
        );
        assert_eq!(
            combined
                .required_limits()
                .min_storage_buffer_offset_alignment,
            64
        );
        assert_eq!(combined.required_limits().max_immediate_size, 0);
    }

    #[test]
    fn device_descriptor_contains_the_exact_request_and_stock_baseline() {
        let requirements = WgpuDeviceRequirements::new(
            wgpu::Features::TIMESTAMP_QUERY,
            wgpu::Limits {
                max_texture_dimension_2d: 1,
                min_uniform_buffer_offset_alignment: 1024,
                ..Default::default()
            },
        );
        assert_eq!(requirements.required_limits(), &wgpu::Limits::default());
        let descriptor = requirements.descriptor();
        assert_eq!(
            descriptor.required_features,
            wgpu::Features::TIMESTAMP_QUERY
        );
        assert_eq!(&descriptor.required_limits, requirements.required_limits());
        assert!(!descriptor.experimental_features.is_enabled());
    }

    #[test]
    fn adapter_support_does_not_prove_requested_features_are_enabled() {
        let requirements =
            WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
        let supported = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TEXTURE_COMPRESSION_BC;
        assert_eq!(
            requirements.validate_adapter(supported, &wgpu::Limits::default()),
            Ok(())
        );
        assert_eq!(
            requirements.validate_enabled(wgpu::Features::empty(), &wgpu::Limits::default()),
            Err(WgpuDeviceRequirementsError::RequiredFeaturesNotEnabled {
                missing: wgpu::Features::TIMESTAMP_QUERY,
            })
        );
        assert_eq!(
            requirements.validate_late_request(
                supported,
                &wgpu::Limits::default(),
                wgpu::Features::empty(),
                &wgpu::Limits::default(),
            ),
            Err(WgpuDeviceRequirementsError::ReconstructionRequired)
        );
        assert_eq!(
            requirements.validate_late_request(
                wgpu::Features::empty(),
                &wgpu::Limits::default(),
                wgpu::Features::empty(),
                &wgpu::Limits::default(),
            ),
            Err(WgpuDeviceRequirementsError::UnsupportedFeatures {
                missing: wgpu::Features::TIMESTAMP_QUERY,
            })
        );
    }

    #[test]
    fn late_limit_request_requires_enabled_capacity_and_alignment() {
        let requirements = WgpuDeviceRequirements::new(
            wgpu::Features::empty(),
            wgpu::Limits {
                max_texture_dimension_2d: 16384,
                min_uniform_buffer_offset_alignment: 64,
                ..Default::default()
            },
        );
        let supported = requirements.required_limits().clone();
        let enabled = wgpu::Limits::default();
        assert_eq!(
            requirements.validate_enabled(wgpu::Features::empty(), &enabled),
            Err(WgpuDeviceRequirementsError::RequiredLimitNotEnabled {
                name: "max_texture_dimension_2d",
                required: 16384,
                enabled: u64::from(enabled.max_texture_dimension_2d),
            })
        );
        assert_eq!(
            requirements.validate_late_request(
                wgpu::Features::empty(),
                &supported,
                wgpu::Features::empty(),
                &enabled,
            ),
            Err(WgpuDeviceRequirementsError::ReconstructionRequired)
        );
        let enabled = wgpu::Limits {
            max_texture_dimension_2d: 32768,
            min_uniform_buffer_offset_alignment: 128,
            ..Default::default()
        };
        assert_eq!(
            requirements.validate_enabled(wgpu::Features::empty(), &enabled),
            Err(WgpuDeviceRequirementsError::RequiredLimitNotEnabled {
                name: "min_uniform_buffer_offset_alignment",
                required: 64,
                enabled: 128,
            })
        );
        let enabled = wgpu::Limits {
            min_uniform_buffer_offset_alignment: 32,
            ..enabled
        };
        assert!(requirements.is_satisfied_by(wgpu::Features::empty(), &enabled));
    }

    #[test]
    fn unsupported_limits_report_wgpu_direction_and_exact_values() {
        let requirements = WgpuDeviceRequirements::new(
            wgpu::Features::empty(),
            wgpu::Limits {
                min_storage_buffer_offset_alignment: 64,
                ..Default::default()
            },
        );
        assert_eq!(
            requirements.validate_adapter(wgpu::Features::empty(), &wgpu::Limits::default()),
            Err(WgpuDeviceRequirementsError::UnsupportedLimit {
                name: "min_storage_buffer_offset_alignment",
                required: 64,
                supported: 256,
            })
        );
        let capacity = WgpuDeviceRequirements::new(
            wgpu::Features::empty(),
            wgpu::Limits {
                max_buffer_size: 512 * 1024 * 1024,
                ..Default::default()
            },
        );
        assert_eq!(
            capacity.validate_adapter(wgpu::Features::empty(), &wgpu::Limits::default()),
            Err(WgpuDeviceRequirementsError::UnsupportedLimit {
                name: "max_buffer_size",
                required: 512 * 1024 * 1024,
                supported: wgpu::Limits::default().max_buffer_size,
            })
        );
    }

    #[test]
    fn late_enabled_subset_is_admitted_without_requesting_adapter_extras() {
        let requirements =
            WgpuDeviceRequirements::new(wgpu::Features::TIMESTAMP_QUERY, wgpu::Limits::default());
        let enabled = requirements.union(&WgpuDeviceRequirements::new(
            wgpu::Features::TEXTURE_COMPRESSION_BC,
            wgpu::Limits {
                max_buffer_size: 512 * 1024 * 1024,
                min_storage_buffer_offset_alignment: 64,
                ..Default::default()
            },
        ));
        assert_eq!(
            requirements.validate_late_request(
                enabled.required_features(),
                enabled.required_limits(),
                enabled.required_features(),
                enabled.required_limits(),
            ),
            Ok(())
        );
        assert_eq!(
            requirements.required_features(),
            wgpu::Features::TIMESTAMP_QUERY
        );
        assert_eq!(requirements.required_limits(), &wgpu::Limits::default());
    }

    #[test]
    fn experimental_feature_declarations_do_not_grant_unsafe_device_permission() {
        let features = wgpu::Features::all_experimental_mask();
        let requirements = WgpuDeviceRequirements::new(features, wgpu::Limits::default());
        assert_eq!(
            requirements.validate_request(),
            Err(WgpuDeviceRequirementsError::ExperimentalFeaturesUnsupported { features })
        );
    }
}
