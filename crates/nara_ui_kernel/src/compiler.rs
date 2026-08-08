use std::{collections::BTreeSet, sync::Arc};

use nara_text::{FontFace, TextEngine, TextEngineCreateError, TextLimits};
use thiserror::Error;

use crate::{
    UiAuthoringSnapshot, UiDiagnostic, UiEnvironment, UiFocusFact, UiFrame, UiFrameRevision,
    UiHitRegion, UiLayer, UiLimits, UiLimitsError, UiPaintPrimitive, UiRootDiagnostic, UiRootKey,
    UiRootOutcome, UiRootStatus, UiSemanticNode, UiTheme,
};

use self::{
    cache::{RootCache, RootVisual},
    order::{FrameOrder, FrameOrdered},
    root::{CompiledRoot, RootBudget, RootCompileError},
};

mod cache;
mod order;
mod root;

/// Stateful owner of text measurement, bounded last-good roots, and frame revisions.
pub struct UiCompiler {
    text_engine: TextEngine,
    limits: UiLimits,
    max_shaped_glyphs: usize,
    next_frame_revision: u64,
    cache_clock: u64,
    cache: RootCache,
}

impl UiCompiler {
    pub fn new(font_face: FontFace, limits: UiLimits) -> Result<Self, UiCompileError> {
        Self::with_text_limits(font_face, limits, TextLimits::default())
    }

    pub fn with_text_limits(
        font_face: FontFace,
        limits: UiLimits,
        text_limits: TextLimits,
    ) -> Result<Self, UiCompileError> {
        limits.validate()?;
        let max_shaped_glyphs = text_limits.max_glyphs;
        Ok(Self {
            text_engine: TextEngine::new(font_face, text_limits)?,
            limits,
            max_shaped_glyphs,
            next_frame_revision: 1,
            cache_clock: 0,
            cache: RootCache::default(),
        })
    }

    /// Compiles all roots independently and publishes one immutable cross-projection frame.
    pub fn compile(
        &mut self,
        snapshot: &UiAuthoringSnapshot,
        environment: UiEnvironment,
        theme: UiTheme,
    ) -> Result<Arc<UiFrame>, UiCompileError> {
        if environment.theme_revision() != theme.revision() {
            return Err(UiCompileError::ThemeRevisionMismatch {
                environment: environment.theme_revision().get(),
                theme: theme.revision().get(),
            });
        }

        let authored_keys = self.preflight_snapshot(snapshot)?;
        let frame_revision = UiFrameRevision::new(self.next_frame_revision);
        let next_frame_revision = self
            .next_frame_revision
            .checked_add(1)
            .ok_or(UiCompileError::FrameRevisionExhausted)?;
        let next_cache_clock = self
            .cache_clock
            .checked_add(1)
            .ok_or(UiCompileError::CacheRevisionExhausted)?;

        let mut candidates = Vec::with_capacity(snapshot.envelopes.len());
        let mut touched = BTreeSet::new();
        let mut node_count = 0usize;
        let mut shaped_glyphs = 0usize;
        let mut paint_count = 0usize;

        for envelope in &snapshot.envelopes {
            let compiled = match envelope.body() {
                crate::authoring::UiRootEnvelopeBody::Ready(index) => root::compile_root(
                    &mut self.text_engine,
                    &snapshot.roots[*index],
                    environment,
                    theme,
                    self.limits,
                    RootBudget::new(
                        shaped_glyphs,
                        self.max_shaped_glyphs,
                        paint_count,
                        self.limits.max_paint_primitives,
                    ),
                ),
                crate::authoring::UiRootEnvelopeBody::Rejected(diagnostic) => {
                    Err(RootCompileError::Local(diagnostic.clone()))
                }
            };

            let (status, diagnostic, body) = match compiled {
                Ok(compiled) => {
                    account_visual(
                        &mut node_count,
                        &mut shaped_glyphs,
                        &mut paint_count,
                        &compiled.visual,
                        self.limits,
                        self.max_shaped_glyphs,
                    )?;
                    (
                        UiRootStatus::Published,
                        None,
                        CandidateBody::Published(compiled),
                    )
                }
                Err(RootCompileError::ShapedGlyphLimitExceeded { actual, maximum }) => {
                    return Err(UiCompileError::ShapedGlyphLimitExceeded { actual, maximum });
                }
                Err(RootCompileError::LogicalPaintLimitExceeded { actual, maximum }) => {
                    return Err(UiCompileError::LogicalPaintLimitExceeded { actual, maximum });
                }
                Err(RootCompileError::Local(diagnostic)) => {
                    let fallback = self.cache.compatible(envelope.key(), environment);
                    if let Some(visual) = fallback {
                        account_visual(
                            &mut node_count,
                            &mut shaped_glyphs,
                            &mut paint_count,
                            &visual,
                            self.limits,
                            self.max_shaped_glyphs,
                        )?;
                        touched.insert(envelope.key().clone());
                        (
                            UiRootStatus::RetainedVisual,
                            Some(diagnostic),
                            CandidateBody::Retained(visual),
                        )
                    } else if envelope.layer() == UiLayer::Modal {
                        return Err(UiCompileError::ModalFallbackUnavailable {
                            key: envelope.key().clone(),
                        });
                    } else {
                        (
                            UiRootStatus::Absent,
                            Some(diagnostic),
                            CandidateBody::Absent,
                        )
                    }
                }
            };
            candidates.push(CandidateRoot {
                key: envelope.key().clone(),
                layer: envelope.layer(),
                ordinal: envelope.ordinal(),
                status,
                diagnostic,
                body,
            });
        }

        let mut layouts = Vec::new();
        let mut paint = Vec::new();
        let mut hits = Vec::new();
        let mut focus = Vec::new();
        let mut semantics = Vec::new();
        let mut outcomes = Vec::with_capacity(candidates.len());

        for candidate in &candidates {
            outcomes.push(UiRootOutcome {
                key: candidate.key.clone(),
                layer: candidate.layer,
                status: candidate.status,
                diagnostic: candidate.diagnostic.clone(),
            });
            match &candidate.body {
                CandidateBody::Published(compiled) => {
                    bind_visual(
                        &mut layouts,
                        &mut paint,
                        &compiled.visual,
                        candidate.layer,
                        candidate.ordinal,
                    );
                    bind_interactive(
                        &mut hits,
                        &mut focus,
                        &mut semantics,
                        compiled,
                        candidate.layer,
                        candidate.ordinal,
                    );
                }
                CandidateBody::Retained(visual) => {
                    bind_visual(
                        &mut layouts,
                        &mut paint,
                        visual,
                        candidate.layer,
                        candidate.ordinal,
                    );
                }
                CandidateBody::Absent => {}
            }
        }

        layouts.sort_by(|left, right| left.order.cmp(&right.order));
        paint.sort_by(|left, right| left.order.cmp(&right.order));
        hits.sort_by(|left, right| right.order.cmp(&left.order));
        focus.sort_by(|left, right| left.order.cmp(&right.order));
        semantics.sort_by(|left, right| left.order.cmp(&right.order));

        let diagnostics = bounded_diagnostics(&outcomes, self.limits.max_diagnostics);
        let frame = Arc::new(UiFrame {
            revision: frame_revision,
            source_revision: snapshot.source_revision,
            environment,
            root_outcomes: outcomes,
            layouts: layouts.into_iter().map(|entry| entry.value).collect(),
            paint: paint.into_iter().map(|entry| entry.value).collect(),
            hits: hits.into_iter().map(|entry| entry.value).collect(),
            focus: focus.into_iter().map(|entry| entry.value).collect(),
            semantics: semantics.into_iter().map(|entry| entry.value).collect(),
            diagnostics,
        });

        let published = candidates.iter().filter_map(|candidate| {
            let CandidateBody::Published(compiled) = &candidate.body else {
                return None;
            };
            Some((
                candidate.key.clone(),
                environment,
                Arc::clone(&compiled.visual),
            ))
        });
        self.cache.commit(
            next_cache_clock,
            self.limits.max_cache_roots,
            &authored_keys,
            &touched,
            published,
        );
        self.next_frame_revision = next_frame_revision;
        self.cache_clock = next_cache_clock;
        Ok(frame)
    }

    fn preflight_snapshot(
        &self,
        snapshot: &UiAuthoringSnapshot,
    ) -> Result<BTreeSet<UiRootKey>, UiCompileError> {
        if snapshot.envelopes.len() > self.limits.max_roots {
            return Err(UiCompileError::SnapshotRootLimitExceeded {
                actual: snapshot.envelopes.len(),
                maximum: self.limits.max_roots,
            });
        }
        let mut authored_keys = BTreeSet::new();
        let mut ready_indices = BTreeSet::new();
        let mut node_count = 0usize;
        for (expected_ordinal, envelope) in snapshot.envelopes.iter().enumerate() {
            if envelope.ordinal() != expected_ordinal {
                return Err(UiCompileError::SnapshotOrderInvalid {
                    expected: expected_ordinal,
                    actual: envelope.ordinal(),
                });
            }
            if !authored_keys.insert(envelope.key().clone()) {
                return Err(UiCompileError::SnapshotDuplicateRootKey {
                    key: envelope.key().clone(),
                });
            }
            if let crate::authoring::UiRootEnvelopeBody::Ready(index) = envelope.body() {
                let Some(root) = snapshot.roots.get(*index) else {
                    return Err(UiCompileError::SnapshotEnvelopeMismatch {
                        ordinal: expected_ordinal,
                    });
                };
                if !ready_indices.insert(*index)
                    || root.key() != envelope.key()
                    || root.layer() != envelope.layer()
                {
                    return Err(UiCompileError::SnapshotEnvelopeMismatch {
                        ordinal: expected_ordinal,
                    });
                }
            }
            if envelope.key().encoded_len() > self.limits.max_key_bytes {
                return Err(UiCompileError::SnapshotKeyLimitExceeded {
                    actual: envelope.key().encoded_len(),
                    maximum: self.limits.max_key_bytes,
                });
            }
            node_count = node_count.saturating_add(envelope.node_count());
            if node_count > self.limits.max_nodes {
                return Err(UiCompileError::SnapshotNodeLimitExceeded {
                    actual: node_count,
                    maximum: self.limits.max_nodes,
                });
            }
        }
        if ready_indices.len() != snapshot.roots.len() {
            return Err(UiCompileError::SnapshotEnvelopeMismatch {
                ordinal: snapshot.envelopes.len(),
            });
        }
        Ok(authored_keys)
    }
}

fn account_visual(
    node_count: &mut usize,
    shaped_glyphs: &mut usize,
    paint_count: &mut usize,
    visual: &RootVisual,
    limits: UiLimits,
    max_shaped_glyphs: usize,
) -> Result<(), UiCompileError> {
    *node_count = node_count.saturating_add(visual.node_count());
    if *node_count > limits.max_nodes {
        return Err(UiCompileError::CandidateNodeLimitExceeded {
            actual: *node_count,
            maximum: limits.max_nodes,
        });
    }
    *shaped_glyphs = shaped_glyphs.saturating_add(visual.shaped_glyphs);
    if *shaped_glyphs > max_shaped_glyphs {
        return Err(UiCompileError::ShapedGlyphLimitExceeded {
            actual: *shaped_glyphs,
            maximum: max_shaped_glyphs,
        });
    }
    *paint_count = paint_count.saturating_add(visual.paint_count());
    if *paint_count > limits.max_paint_primitives {
        return Err(UiCompileError::LogicalPaintLimitExceeded {
            actual: *paint_count,
            maximum: limits.max_paint_primitives,
        });
    }
    Ok(())
}

fn bind_visual(
    layouts: &mut Vec<FrameOrdered<crate::UiLayoutFact>>,
    paint: &mut Vec<FrameOrdered<UiPaintPrimitive>>,
    visual: &RootVisual,
    layer: UiLayer,
    root_ordinal: usize,
) {
    for entry in &visual.layouts {
        layouts.push(FrameOrdered::new(
            FrameOrder::bind(layer, root_ordinal, &entry.order),
            entry.value.bind(layer),
        ));
    }
    for entry in &visual.paint {
        paint.push(FrameOrdered::new(
            FrameOrder::bind(layer, root_ordinal, &entry.order),
            entry.value.clone(),
        ));
    }
}

fn bind_interactive(
    hits: &mut Vec<FrameOrdered<UiHitRegion>>,
    focus: &mut Vec<FrameOrdered<UiFocusFact>>,
    semantics: &mut Vec<FrameOrdered<UiSemanticNode>>,
    compiled: &CompiledRoot,
    layer: UiLayer,
    root_ordinal: usize,
) {
    for entry in &compiled.hits {
        hits.push(FrameOrdered::new(
            FrameOrder::bind(layer, root_ordinal, &entry.order),
            entry.value.clone(),
        ));
    }
    for entry in &compiled.focus {
        focus.push(FrameOrdered::new(
            FrameOrder::bind(layer, root_ordinal, &entry.order),
            entry.value.clone(),
        ));
    }
    for entry in &compiled.semantics {
        semantics.push(FrameOrdered::new(
            FrameOrder::bind(layer, root_ordinal, &entry.order),
            entry.value.clone(),
        ));
    }
}

fn bounded_diagnostics(outcomes: &[UiRootOutcome], maximum: usize) -> Vec<UiDiagnostic> {
    let mut diagnostics = outcomes
        .iter()
        .filter_map(|outcome| {
            outcome.diagnostic.clone().map(|detail| UiDiagnostic::Root {
                key: outcome.key.clone(),
                detail,
            })
        })
        .collect::<Vec<_>>();
    if diagnostics.len() > maximum {
        let omitted = diagnostics.len() - maximum + 1;
        diagnostics.truncate(maximum - 1);
        diagnostics.push(UiDiagnostic::Truncated { omitted });
    }
    diagnostics
}

#[derive(Debug)]
enum CandidateBody {
    Published(CompiledRoot),
    Retained(Arc<RootVisual>),
    Absent,
}

#[derive(Debug)]
struct CandidateRoot {
    key: UiRootKey,
    layer: UiLayer,
    ordinal: usize,
    status: UiRootStatus,
    diagnostic: Option<UiRootDiagnostic>,
    body: CandidateBody,
}

#[derive(Debug, Error)]
pub enum UiCompileError {
    #[error(transparent)]
    Limits(#[from] UiLimitsError),
    #[error(transparent)]
    TextEngine(#[from] TextEngineCreateError),
    #[error("UI environment theme revision {environment} does not match theme revision {theme}")]
    ThemeRevisionMismatch { environment: u64, theme: u64 },
    #[error("UI snapshot has {actual} roots; compiler maximum is {maximum}")]
    SnapshotRootLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot has {actual} nodes; compiler maximum is {maximum}")]
    SnapshotNodeLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot root key has {actual} encoded bytes; compiler maximum is {maximum}")]
    SnapshotKeyLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot contains duplicate root key {key:?}")]
    SnapshotDuplicateRootKey { key: UiRootKey },
    #[error("UI snapshot root ordinal is {actual}; expected {expected}")]
    SnapshotOrderInvalid { expected: usize, actual: usize },
    #[error("UI snapshot root envelope at ordinal {ordinal} does not match its ready body")]
    SnapshotEnvelopeMismatch { ordinal: usize },
    #[error("UI candidate has {actual} nodes; compiler maximum is {maximum}")]
    CandidateNodeLimitExceeded { actual: usize, maximum: usize },
    #[error("UI candidate has {actual} shaped glyphs; maximum is {maximum}")]
    ShapedGlyphLimitExceeded { actual: usize, maximum: usize },
    #[error("UI candidate has {actual} logical paint primitives; maximum is {maximum}")]
    LogicalPaintLimitExceeded { actual: usize, maximum: usize },
    #[error("modal root {key:?} has no compatible retained visual or barrier")]
    ModalFallbackUnavailable { key: UiRootKey },
    #[error("UI frame revision is exhausted")]
    FrameRevisionExhausted,
    #[error("UI cache revision is exhausted")]
    CacheRevisionExhausted,
}
