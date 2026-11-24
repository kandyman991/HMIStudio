#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Kind { Button, Text, Gauge }

#[derive(Clone, Debug, Serialize)]
struct Component { id: u32, kind: Kind, label: String }

#[derive(Clone, Debug, Serialize)]
struct Snapshot { version: &'static str, components: Vec<Component>, selected: Option<u32> }

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Command { Snapshot, Insert { kind: Kind }, Select { id: u32 } }

struct Engine { next_id: u32, components: Vec<Component>, selected: Option<u32> }
impl Engine {
    fn new() -> Self {
        Self { next_id: 3, components: vec![
            Component { id: 1, kind: Kind::Gauge, label: "Conveyor speed".into() },
            Component { id: 2, kind: Kind::Button, label: "START (demo)".into() },
        ], selected: None }
    }
    fn apply(&mut self, command: Command) -> Result<Snapshot, String> {
        match command {
            Command::Snapshot => {}
            Command::Insert { kind } => {
                if self.components.len() >= 1000 { return Err("Demo component limit reached".into()); }
                let id = self.next_id;
                self.next_id = id.checked_add(1).ok_or("Component ID exhausted")?;
                let label = match kind { Kind::Button => "Button", Kind::Text => "Text", Kind::Gauge => "Gauge" };
                self.components.push(Component { id, kind, label: label.into() });
                self.selected = Some(id);
            }
            Command::Select { id } => {
                if !self.components.iter().any(|c| c.id == id) { return Err("Unknown component".into()); }
                self.selected = Some(id);
            }
        }
        Ok(Snapshot { version: env!("CARGO_PKG_VERSION"), components: self.components.clone(), selected: self.selected })
    }
}
#[tauri::command]
fn engineering_command(command: Command, state: tauri::State<'_, Mutex<Engine>>) -> Result<Snapshot, String> {
    state.lock().map_err(|_| "Engineering core unavailable".to_string())?.apply(command)
}
fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(Engine::new()))
        .invoke_handler(tauri::generate_handler![engineering_command])
        .run(tauri::generate_context!())
        .expect("HMI Studio startup failed");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insert_and_select() {
        let mut engine = Engine::new();
        let snapshot = engine.apply(Command::Insert { kind: Kind::Text }).unwrap();
        assert_eq!(snapshot.selected, Some(3));
        assert_eq!(snapshot.components[2].kind, Kind::Text);
        assert_eq!(engine.apply(Command::Select { id: 1 }).unwrap().selected, Some(1));
    }
    #[test]
    fn invalid_selection_is_non_mutating() {
        let mut engine = Engine::new();
        assert!(engine.apply(Command::Select { id: 999 }).is_err());
        assert_eq!(engine.apply(Command::Snapshot).unwrap().selected, None);
    }
}
