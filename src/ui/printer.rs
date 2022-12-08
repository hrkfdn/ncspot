use cursive::Printer;

/// And extension trait to make usage of the cursive::Printer simpler.
pub trait PrinterExt {
    /// Print at the beginning of the first line of a printer.
    fn print_at_start(&self, text: &str);

    /// Print at the end of the first line of a printer.
    fn print_at_end(&self, text: &str);

    /// Print at the given percentage, taking the text length into account.
    /// This is what you want to use when you have text with equal lengths.
    fn print_at_percent(&self, percent: u32, text: &str);

    /// Print at the given percentage, not taking the text length into account.
    /// This is what you want to use when you want aligned text with varying
    /// lengths.
    fn print_at_percent_absolute(&self, percent: u32, text: &str);
}

impl PrinterExt for Printer<'_, '_> {
    fn print_at_start(&self, text: &str) {
        self.print((0, 0), text);
    }

    fn print_at_end(&self, text: &str) {
        self.print((self.size.x - text.len(), 0), text);
    }

    fn print_at_percent(&self, percent: u32, text: &str) {
        self.print(
            (
                ((self.size.x - text.len()) as f64 * (0.01 * percent.clamp(0, 100) as f64)) as u32,
                0,
            ),
            text,
        )
    }

    fn print_at_percent_absolute(&self, percent: u32, text: &str) {
        self.print(
            (
                (self.size.x as f64 * (0.01 * percent.clamp(0, 100) as f64)) as u32,
                0,
            ),
            text,
        )
    }
}
