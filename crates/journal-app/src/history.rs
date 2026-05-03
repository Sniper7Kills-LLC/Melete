use journal_core::Stroke;

#[derive(Clone)]
pub enum Op {
    AddStroke(Stroke),
    RemoveStroke(Stroke),
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

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
