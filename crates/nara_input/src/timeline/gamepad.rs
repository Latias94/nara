use super::*;

impl InputTimeline {
    /// Connects a normalized platform gamepad to an already connected window and seat.
    ///
    /// The backend label is metadata, including when two connections use the same label.
    /// Reconnecting creates a fresh generation and does not recover former physical state.
    pub fn connect_gamepad(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        raw_label: u16,
    ) -> Result<GamepadConnection, InputIngressError> {
        self.connect_gamepad_origin(window, seat, raw_label, InputEventOrigin::Platform)
            .map(|(connection, _)| connection)
    }

    /// Connects a synthetic stream through the same physical membership and terminal owner.
    ///
    /// The origin is captured for the entire connection, including its terminal records.
    pub fn connect_synthetic_gamepad(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        raw_label: u16,
    ) -> Result<GamepadConnection, InputIngressError> {
        self.connect_gamepad_origin(window, seat, raw_label, InputEventOrigin::Synthetic)
            .map(|(connection, _)| connection)
    }

    pub(super) fn connect_gamepad_origin(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        raw_label: u16,
        origin: InputEventOrigin,
    ) -> Result<(GamepadConnection, InputAdmission), InputIngressError> {
        let endpoint_index = self.topology.connected_endpoint(window, seat)?;
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let slot = self.topology.free_gamepad()?;
        let identity = self.topology.next_identity()?;
        let after = self.topology.obligations() - endpoint.obligations()
            + endpoint.rearmed_obligations()
            + 1;
        let plan = self.plan(1, after)?;
        let draft = self.ordinary_draft(
            window,
            seat,
            None,
            InputEventKind::GamepadConnected {
                gamepad: raw_label,
                connection_generation: InputGamepadGeneration(identity),
            },
            origin,
        );
        let admission = self.publish(plan, &[draft]);
        let gamepad = self
            .topology
            .install_gamepad(slot, identity, endpoint.id, raw_label, origin);
        self.topology.endpoints[endpoint_index]
            .as_mut()
            .unwrap()
            .rearm();
        self.assert_accounting();
        Ok((
            GamepadConnection {
                owner: Arc::clone(&self.owner),
                endpoint: endpoint.id,
                gamepad,
                raw_label,
                window,
                seat,
            },
            admission,
        ))
    }

    fn connected_gamepad(
        &self,
        connection: &GamepadConnection,
    ) -> Result<(usize, usize), InputIngressError> {
        if !Arc::ptr_eq(&self.owner, &connection.owner) {
            return Err(InputIngressError::ForeignGamepadConnection);
        }
        let expired = InputIngressError::GamepadConnectionExpired {
            connection_generation: connection.connection_generation(),
        };
        let endpoint_index = self
            .topology
            .connected_endpoint(connection.window, connection.seat)?;
        if self.topology.endpoints[endpoint_index].unwrap().id != connection.endpoint {
            return Err(expired);
        }
        let gamepad_index = self
            .topology
            .gamepads
            .iter()
            .position(|slot| {
                slot.is_some_and(|gamepad| {
                    gamepad.id == connection.gamepad
                        && gamepad.endpoint == connection.endpoint
                        && !gamepad.retiring
                })
            })
            .ok_or(expired)?;
        Ok((endpoint_index, gamepad_index))
    }

    /// Releases accepted buttons, neutralizes accepted axes, then disconnects in one ordered batch.
    ///
    /// Every record spends an existing terminal credit, even when ordinary admission is full.
    /// The retired slot remains charged until its original internal consumers have finished.
    pub fn disconnect_gamepad(
        &mut self,
        connection: &GamepadConnection,
    ) -> Result<InputAdmission, InputIngressError> {
        let (_, gamepad_index) = self.connected_gamepad(connection)?;
        let gamepad = self.topology.gamepads[gamepad_index].unwrap();
        self.with_drafts(|owner, drafts| {
            for member in owner
                .topology
                .members
                .iter()
                .flatten()
                .copied()
                .filter(|member| member.pressed && member.gamepad == Some(gamepad.id))
            {
                drafts.push(
                    owner.release_draft(member, Some(InputInvalidationReason::GamepadDisconnected)),
                );
            }
            drafts.sort_by_key(|draft| draft.press_origin.unwrap().0);
            let mut disconnected = owner.ordinary_draft(
                connection.window,
                connection.seat,
                None,
                InputEventKind::GamepadDisconnected {
                    gamepad: gamepad.raw_label,
                    connection_generation: gamepad.id.0,
                },
                gamepad.origin,
            );
            disconnected.invalidation = Some(InputInvalidationReason::GamepadDisconnected);
            drafts.push(disconnected);
            let plan =
                owner.terminal_plan(drafts.len(), owner.topology.obligations() - drafts.len());
            let admission = owner.publish(plan, drafts);
            for member in owner
                .topology
                .members
                .iter_mut()
                .flatten()
                .filter(|member| member.gamepad == Some(gamepad.id))
            {
                member.pressed = false;
                if let Some(value) = &mut member.axis_value {
                    *value = AxisValue::ZERO;
                }
            }
            owner.topology.gamepads[gamepad_index]
                .as_mut()
                .unwrap()
                .retiring = true;
            owner.assert_accounting();
            owner.reclaim_completed();
            Ok(admission)
        })
    }

    /// Admits a button edge using the origin and endpoint captured by the connection.
    pub fn admit_gamepad_button(
        &mut self,
        connection: &GamepadConnection,
        button: u16,
        phase: ButtonTransitionPhase,
    ) -> Result<InputAdmission, InputIngressError> {
        let (endpoint_index, gamepad_index) = self.connected_gamepad(connection)?;
        let gamepad = self.topology.gamepads[gamepad_index].unwrap();
        let output = match phase {
            ButtonTransitionPhase::Pressed => self.press(
                endpoint_index,
                None,
                Some(gamepad),
                InputBinding::GamepadButton(button),
                gamepad.origin,
                None,
            ),
            ButtonTransitionPhase::Released => self.release(
                endpoint_index,
                None,
                Some(gamepad.id),
                InputBinding::GamepadButton(button),
            ),
        }?;
        Ok(output.admission)
    }

    /// Admits one validated normalized axis sample without applying an implicit dead zone.
    ///
    /// A nonzero sample starts one member. Changes keep its original press and route; zero
    /// terminates it using the already reserved neutral credit. Identical samples are ignored.
    pub fn admit_gamepad_axis(
        &mut self,
        connection: &GamepadConnection,
        axis: u16,
        value: AxisValue,
    ) -> Result<InputAdmission, InputIngressError> {
        let (endpoint_index, gamepad_index) = self.connected_gamepad(connection)?;
        let gamepad = self.topology.gamepads[gamepad_index].unwrap();
        let input = InputBinding::GamepadAxis(axis);
        let member_index =
            self.topology
                .member_index(connection.endpoint, None, Some(gamepad.id), input);
        if value == AxisValue::ZERO {
            return self
                .release(endpoint_index, None, Some(gamepad.id), input)
                .map(|output| output.admission);
        }
        let Some(member_index) = member_index else {
            return self
                .press(
                    endpoint_index,
                    None,
                    Some(gamepad),
                    input,
                    gamepad.origin,
                    Some(value),
                )
                .map(|output| output.admission);
        };
        let member = self.topology.members[member_index].unwrap();
        if member.axis_value == Some(value) {
            return Ok(InputAdmission::IGNORED);
        }
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let after =
            self.topology.obligations() - endpoint.obligations() + endpoint.rearmed_obligations();
        let plan = self.plan(1, after)?;
        let draft = self.member_draft(member, value, None);
        let admission = self.publish(plan, &[draft]);
        self.topology.members[member_index]
            .as_mut()
            .unwrap()
            .axis_value = Some(value);
        self.topology.endpoints[endpoint_index]
            .as_mut()
            .unwrap()
            .rearm();
        self.assert_accounting();
        Ok(admission)
    }
}
