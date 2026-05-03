use journal_core::Stroke;
use uuid::Uuid;

#[derive(Clone)]
pub enum Op {
    AddStroke(Stroke),
    RemoveStroke(Stroke),
    MoveStrokes { ids: Vec<Uuid>, dx: f64, dy: f64 },
}

pub struct History {
    pub undo: Vec<Op>,
    pub redo: Vec<Op>,
}

impl History {
    pub fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new() }
    }

    pub fn push_add(&mut self, stroke: Stroke) {
        self.undo.push(Op::AddStroke(stroke));
        self.redo.clear();
    }

    pub fn push_remove(&mut self, stroke: Stroke) {
        self.undo.push(Op::RemoveStroke(stroke));
        self.redo.clear();
    }

    pub fn push_move(&mut self, ids: Vec<Uuid>, dx: f64, dy: f64) {
        if ids.is_empty() || (dx.abs() < 1e-9 && dy.abs() < 1e-9) {
            return;
        }
        self.undo.push(Op::MoveStrokes { ids, dx, dy });
        self.redo.clear();
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
