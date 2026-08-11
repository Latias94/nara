use nara_ui_kernel::{UiRevision, UiSemanticSnapshot, UiSurfaceId, UiSurfaceRetirementToken};

use super::WinitPlatformAccessibilityError;

/// Exact lifecycle authority retained while one native target has no published tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VacantPlatformAuthority {
    Invalidated {
        surface_id: UiSurfaceId,
        floor: UiRevision,
    },
    Suspended {
        surface_id: UiSurfaceId,
        floor: UiRevision,
    },
    Detached {
        surface_id: UiSurfaceId,
        floor: UiRevision,
    },
    Retired(UiSurfaceRetirementToken),
}

impl VacantPlatformAuthority {
    const fn surface_id(self) -> UiSurfaceId {
        match self {
            Self::Invalidated { surface_id, .. }
            | Self::Suspended { surface_id, .. }
            | Self::Detached { surface_id, .. } => surface_id,
            Self::Retired(retirement) => retirement.surface_id(),
        }
    }

    const fn floor(self) -> UiRevision {
        match self {
            Self::Invalidated { floor, .. }
            | Self::Suspended { floor, .. }
            | Self::Detached { floor, .. } => floor,
            Self::Retired(retirement) => retirement.slot_generation(),
        }
    }

    pub(super) fn validate_snapshot(
        self,
        snapshot: &UiSemanticSnapshot,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        self.validate_route(snapshot.surface_id(), snapshot.route().slot_generation())
    }

    fn validate_route(
        self,
        actual: UiSurfaceId,
        candidate: UiRevision,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        let expected = self.surface_id();
        if actual != expected {
            return match self {
                Self::Detached { .. } | Self::Retired(_) => Ok(()),
                Self::Invalidated { .. } | Self::Suspended { .. } => {
                    Err(WinitPlatformAccessibilityError::VacantSurfaceMismatch { expected, actual })
                }
            };
        }
        if matches!(self, Self::Retired(_)) {
            return Err(WinitPlatformAccessibilityError::RetiredSurfaceReused {
                surface: actual.get(),
            });
        }
        let floor = self.floor();
        let valid = match self {
            Self::Suspended { .. } => candidate > floor,
            Self::Invalidated { .. } | Self::Detached { .. } => candidate >= floor,
            Self::Retired(_) => false,
        };
        if !valid {
            return Err(
                WinitPlatformAccessibilityError::SnapshotPredatesVacantTransition {
                    floor,
                    candidate,
                },
            );
        }
        Ok(())
    }

    pub(super) fn advance(self, next: Self) -> Result<Self, WinitPlatformAccessibilityError> {
        if self.surface_id() != next.surface_id() {
            return match self {
                Self::Detached { .. } | Self::Retired(_) => Ok(next),
                Self::Invalidated { .. } | Self::Suspended { .. } => {
                    Err(WinitPlatformAccessibilityError::VacantSurfaceMismatch {
                        expected: self.surface_id(),
                        actual: next.surface_id(),
                    })
                }
            };
        }
        if let Self::Retired(committed) = self {
            return if next == Self::Retired(committed) {
                Ok(self)
            } else {
                Err(WinitPlatformAccessibilityError::RetiredSurfaceReused {
                    surface: committed.surface_id().get(),
                })
            };
        }
        if next == self {
            return Ok(self);
        }
        if next.floor() <= self.floor() {
            return Err(WinitPlatformAccessibilityError::StaleVacantTransition {
                current: self.floor(),
                candidate: next.floor(),
            });
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_floors_reject_stale_routes_without_a_publisher() {
        let surface = UiSurfaceId::new(1).unwrap();
        let other = UiSurfaceId::new(2).unwrap();
        let floor = UiRevision::new(3).unwrap();
        let stale = UiRevision::new(2).unwrap();

        let invalidated = VacantPlatformAuthority::Invalidated {
            surface_id: surface,
            floor,
        };
        assert!(matches!(
            invalidated.validate_route(surface, stale),
            Err(WinitPlatformAccessibilityError::SnapshotPredatesVacantTransition {
                floor: actual_floor,
                candidate,
            }) if actual_floor == floor && candidate == stale
        ));
        assert!(matches!(
            invalidated.validate_route(other, floor),
            Err(WinitPlatformAccessibilityError::VacantSurfaceMismatch {
                expected,
                actual,
            }) if expected == surface && actual == other
        ));

        let suspended = VacantPlatformAuthority::Suspended {
            surface_id: surface,
            floor,
        };
        assert!(suspended.validate_route(surface, floor).is_err());
        assert!(
            suspended
                .validate_route(surface, UiRevision::new(4).unwrap())
                .is_ok()
        );

        let detached = VacantPlatformAuthority::Detached {
            surface_id: surface,
            floor,
        };
        assert!(detached.validate_route(other, stale).is_ok());
        assert!(detached.validate_route(surface, stale).is_err());
        assert!(detached.validate_route(surface, floor).is_ok());

        assert!(matches!(
            suspended.advance(invalidated),
            Err(WinitPlatformAccessibilityError::StaleVacantTransition {
                current,
                candidate,
            }) if current == floor && candidate == floor
        ));
        assert_eq!(suspended.advance(suspended).unwrap(), suspended);
        let successor_suspension = detached
            .advance(VacantPlatformAuthority::Suspended {
                surface_id: other,
                floor: stale,
            })
            .unwrap();
        assert!(successor_suspension.validate_route(other, stale).is_err());
        assert!(successor_suspension.validate_route(other, floor).is_ok());
    }

    #[test]
    fn retirement_rejects_only_the_retired_surface() {
        let surface = UiSurfaceId::new(3).unwrap();
        let other = UiSurfaceId::new(4).unwrap();
        let mut journal = nara_ui_kernel::UiFrameJournal::default();
        journal.install_surface(surface, 1).unwrap();
        let retirement = journal.retire_surface(surface, 2).unwrap().unwrap();
        let vacant = VacantPlatformAuthority::Retired(retirement);

        assert!(matches!(
            vacant.validate_route(surface, UiRevision::new(3).unwrap()),
            Err(WinitPlatformAccessibilityError::RetiredSurfaceReused { surface: 3 })
        ));
        assert!(
            vacant
                .validate_route(other, UiRevision::new(1).unwrap())
                .is_ok()
        );
        let successor = vacant
            .advance(VacantPlatformAuthority::Invalidated {
                surface_id: other,
                floor: UiRevision::new(1).unwrap(),
            })
            .unwrap();
        assert!(
            successor
                .validate_route(other, UiRevision::new(1).unwrap())
                .is_ok()
        );
    }
}
