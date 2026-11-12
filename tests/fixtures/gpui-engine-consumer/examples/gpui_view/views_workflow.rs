//! Domain, background evaluation and exact submitted View composition in the original Host.

use super::{Viewer, engine::views::ViewKey, graph_panel::GraphIntent, views_panel::ViewsIntent};
use anyhow::{Result, ensure};
use nara_gpui_engine_consumer::graph::preview::GraphPreviewBasis;
use std::{rc::Rc, sync::Arc};

impl Viewer {
    fn require_graph_basis(&self, basis: GraphPreviewBasis) -> Result<()> {
        let snapshot = self.engine.as_ref().unwrap().borrow().graph_snapshot()?;
        ensure!(
            snapshot.document == basis.document && snapshot.revision == basis.revision,
            "the displayed graph source changed before the operation"
        );
        Ok(())
    }

    pub(super) fn drive_views(&mut self) -> Result<()> {
        let mut compile = false;
        loop {
            let Some(intent) = self.views_commands.borrow_mut().pop_front() else {
                break;
            };
            let result = (|| -> Result<()> {
                match intent {
                    ViewsIntent::Edit(basis) => {
                        self.require_graph_basis(basis)?;
                        self.engine
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .increment_graph()?;
                    }
                    ViewsIntent::Compile(basis) => {
                        self.require_graph_basis(basis)?;
                        compile = true;
                    }
                    ViewsIntent::HoldNext => self.preview_jobs.as_mut().unwrap().hold_next(),
                    ViewsIntent::ReleaseHeld => {
                        self.preview_jobs.as_mut().unwrap().release_held()?
                    }
                    ViewsIntent::RelocateA => self
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .redisplay_view(ViewKey::A)?,
                    ViewsIntent::PulseB => self
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .pulse_view(ViewKey::B)?,
                    ViewsIntent::CloseA => self
                        .engine
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .close_view(ViewKey::A)?,
                    ViewsIntent::Save(basis) | ViewsIntent::Reopen(basis) => {
                        self.require_graph_basis(basis)?;
                        let intent = if matches!(intent, ViewsIntent::Save(_)) {
                            GraphIntent::Save {
                                document: basis.document,
                                revision: basis.revision,
                            }
                        } else {
                            GraphIntent::Reopen {
                                document: basis.document,
                                revision: basis.revision,
                            }
                        };
                        self.engine
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .graph_intent(intent)?;
                    }
                    ViewsIntent::CloseGraph(basis) => {
                        self.require_graph_basis(basis)?;
                        self.engine
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .close_graph_document()?;
                    }
                    ViewsIntent::Pick { snapshot, point } => {
                        let presentation =
                            snapshot.presented.borrow().clone().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "the displayed View has no current composite authority"
                                )
                            })?;
                        self.engine
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .activate_view(&presentation, point)?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.view_refusals += 1;
                self.views_status = Some(error.to_string());
                println!("NARA_GPUI_VIEW_ACTION_REFUSED {error:#}");
            } else {
                self.views_status = None;
            }
        }
        let snapshot = {
            let mut engine = self.engine.as_ref().unwrap().borrow_mut();
            engine.poll_graph()?;
            engine
                .graph_document
                .map(|_| engine.graph_snapshot())
                .transpose()?
        };
        let basis = snapshot.as_ref().map(|snapshot| GraphPreviewBasis {
            document: snapshot.document,
            revision: snapshot.revision,
        });
        let jobs = self.preview_jobs.as_mut().unwrap();
        let changed = jobs.observe(basis);
        jobs.poll();
        self.compile_requested = basis.is_some() && (self.compile_requested || changed || compile);
        if self.compile_requested
            && jobs.pending_jobs() < 4
            && let Some(snapshot) = snapshot
        {
            let executor = self
                .application
                .as_ref()
                .unwrap()
                .update(|cx| cx.background_executor().clone());
            let proxy = self.proxy.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = proxy.send_event(super::Wake);
            });
            if let Err(error) = jobs.start(snapshot.view, &executor, wake) {
                self.views_status = Some(error.to_string());
            }
            self.compile_requested = false;
        }
        jobs.poll();
        let state = jobs.state();
        if let Some(value) = &state.last_good
            && Some(value.basis) == state.current
            && Some(value.request) != self.published_preview
        {
            self.engine
                .as_ref()
                .unwrap()
                .borrow_mut()
                .publish_views(value)?;
            self.published_preview = Some(value.request);
        }
        let images = self.engine.as_ref().unwrap().borrow().view_images();
        for (index, image) in images.into_iter().enumerate() {
            if let Some(image) = image {
                if !self.view_imports[index]
                    .as_ref()
                    .is_some_and(|(old, _)| Rc::ptr_eq(old, &image))
                {
                    let texture = self
                        .renderer
                        .as_ref()
                        .unwrap()
                        .import_display(&image.display)?;
                    self.view_imports[index] = Some((image, texture));
                }
            } else {
                self.view_imports[index] = None;
            }
        }
        let source = state.current.map_or_else(
            || "Graph closed".to_owned(),
            |basis| format!("Graph revision {}", basis.revision.generation()),
        );
        let last_good = state.last_good.as_ref().map_or_else(
            || "No completed preview".to_owned(),
            |value| {
                let value_text = value
                    .value
                    .map_or_else(|| "not connected".to_owned(), |value| value.to_string());
                format!(
                    "Showing {value_text} from revision {}",
                    value.basis.revision.generation()
                )
            },
        );
        let status = format!(
            "{source} · {} · {last_good}{}",
            if self.compile_requested {
                "Preview waiting for returning jobs"
            } else if state.pending.is_some() {
                "Preview pending"
            } else {
                "Preview settled"
            },
            self.views_status
                .as_ref()
                .or(state.diagnostic.as_ref())
                .map_or(String::new(), |error| format!(" · {error}")),
        );
        let images = self.view_imports.clone();
        self.application.as_ref().unwrap().update(|cx| {
            self.views_panel.as_ref().unwrap().update(cx, |panel, cx| {
                panel.synchronize(images, state.current, status, cx);
            });
        });
        Ok(())
    }
}
