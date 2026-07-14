//! Native setup-command state machine.
//!
//! Tauri capabilities keep setup-only commands away from the main window,
//! while this guard prevents duplicate or out-of-order execution inside the
//! setup process itself. The backend remains the source of truth for whether
//! setup is complete; this module only serializes native side effects.

use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Default)]
struct SetupCommandState {
    initialized: bool,
    completed_at_start: bool,
    integration_in_flight: bool,
    integration_applied: bool,
    handoff_in_flight: bool,
    handoff_completed: bool,
    restart_in_flight: bool,
}

#[derive(Debug, Default)]
pub struct SetupCommandGuard {
    inner: Mutex<SetupCommandState>,
}

impl SetupCommandGuard {
    fn lock(&self) -> Result<MutexGuard<'_, SetupCommandState>, String> {
        self.inner
            .lock()
            .map_err(|_| "the native setup state lock is poisoned".to_owned())
    }

    /// Initialize the guard before the first desktop window is created.
    pub fn initialize(&self, setup_completed: bool) -> Result<(), String> {
        let mut state = self.lock()?;
        state.initialized = true;
        state.completed_at_start = setup_completed;
        Ok(())
    }

    /// Reject setup-only inspection when this process started completed.
    pub fn ensure_setup_window_allowed(&self) -> Result<(), String> {
        let state = self.lock()?;
        if !state.initialized {
            return Err("the native setup state has not been initialized".into());
        }
        if state.completed_at_start {
            return Err("setup was already completed before this process started".into());
        }
        if state.handoff_completed {
            return Err("setup handoff has already completed".into());
        }
        Ok(())
    }

    /// Reserve the integration step. A failed attempt may be retried, while a
    /// successful application is one-shot for the lifetime of this process.
    pub fn begin_integration(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        Self::ensure_active(&state)?;
        if state.integration_in_flight {
            return Err("Windows integration is already running".into());
        }
        if state.integration_applied {
            return Err("Windows integration has already been applied".into());
        }
        if state.handoff_in_flight || state.handoff_completed {
            return Err("Windows integration cannot run after setup handoff begins".into());
        }
        if state.restart_in_flight {
            return Err("Windows integration cannot run while restart is pending".into());
        }
        state.integration_in_flight = true;
        Ok(())
    }

    pub fn finish_integration(&self, completed: bool) -> Result<(), String> {
        let mut state = self.lock()?;
        state.integration_in_flight = false;
        state.integration_applied = completed;
        Ok(())
    }

    /// Reserve the deterministic transition from setup to the main app.
    pub fn begin_handoff(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        Self::ensure_active(&state)?;
        if state.integration_in_flight {
            return Err("setup handoff cannot begin while Windows integration is running".into());
        }
        if state.restart_in_flight {
            return Err("setup handoff cannot begin while restart is pending".into());
        }
        if state.handoff_in_flight || state.handoff_completed {
            return Err("setup handoff has already been requested".into());
        }
        state.handoff_in_flight = true;
        Ok(())
    }

    pub fn finish_handoff(&self, completed: bool) -> Result<(), String> {
        let mut state = self.lock()?;
        state.handoff_in_flight = false;
        state.handoff_completed = completed;
        Ok(())
    }

    /// Reserve a process restart. Failed process creation may be retried.
    pub fn begin_restart(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        Self::ensure_active(&state)?;
        if state.integration_in_flight {
            return Err("Ravyn cannot restart while Windows integration is running".into());
        }
        if state.handoff_in_flight || state.handoff_completed {
            return Err("Ravyn cannot restart after setup handoff begins".into());
        }
        if state.restart_in_flight {
            return Err("a setup restart has already been requested".into());
        }
        state.restart_in_flight = true;
        Ok(())
    }

    pub fn finish_restart(&self, completed: bool) -> Result<(), String> {
        let mut state = self.lock()?;
        state.restart_in_flight = completed;
        Ok(())
    }

    fn ensure_active(state: &SetupCommandState) -> Result<(), String> {
        if !state.initialized {
            return Err("the native setup state has not been initialized".into());
        }
        if state.completed_at_start {
            return Err("setup was already completed before this process started".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_integration_is_one_shot() {
        let guard = SetupCommandGuard::default();
        guard.initialize(false).unwrap();
        guard.begin_integration().unwrap();
        guard.finish_integration(true).unwrap();
        assert!(guard.begin_integration().is_err());
    }

    #[test]
    fn failed_integration_can_be_retried() {
        let guard = SetupCommandGuard::default();
        guard.initialize(false).unwrap();
        guard.begin_integration().unwrap();
        guard.finish_integration(false).unwrap();
        assert!(guard.begin_integration().is_ok());
    }

    #[test]
    fn completed_bootstrap_rejects_setup_commands() {
        let guard = SetupCommandGuard::default();
        guard.initialize(true).unwrap();
        assert!(guard.ensure_setup_window_allowed().is_err());
        assert!(guard.begin_handoff().is_err());
    }

    #[test]
    fn handoff_blocks_restart_and_reentry() {
        let guard = SetupCommandGuard::default();
        guard.initialize(false).unwrap();
        guard.begin_handoff().unwrap();
        assert!(guard.begin_restart().is_err());
        assert!(guard.begin_handoff().is_err());
        guard.finish_handoff(false).unwrap();
        assert!(guard.begin_handoff().is_ok());
    }
}
