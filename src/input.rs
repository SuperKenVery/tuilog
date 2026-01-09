#[derive(Clone, Default)]
pub struct TextInput {
    pub text: String,
    pub cursor: usize,
    pub error: Option<String>,
}

impl TextInput {
    pub fn new(text: String) -> Self {
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            error: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self.char_to_byte_index(self.cursor);
        self.text.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn delete_char_before_cursor(&mut self) {
        if self.cursor > 0 {
            let byte_idx = self.char_to_byte_index(self.cursor - 1);
            self.text.remove(byte_idx);
            self.cursor -= 1;
        }
    }

    pub fn delete_char_at_cursor(&mut self) {
        let len = self.text.chars().count();
        if self.cursor < len {
            let byte_idx = self.char_to_byte_index(self.cursor);
            self.text.remove(byte_idx);
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let len = self.text.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    fn char_to_byte_index(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }
}
