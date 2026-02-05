use std::fmt;

use tracing::field::{Field, Visit};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::{FormatFields, Writer};

pub(crate) fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(false)
        .with_target(false)
        .fmt_fields(MessageOnlyFields)
        .try_init();
}

struct MessageOnlyFields;

impl<'writer> FormatFields<'writer> for MessageOnlyFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = MessageVisitor { writer };
        fields.record(&mut visitor);
        Ok(())
    }
}

struct MessageVisitor<'writer> {
    writer: Writer<'writer>,
}

impl Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() != "message" {
            return;
        }
        let _ = write!(self.writer, "{value:?}");
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() != "message" {
            return;
        }
        let _ = write!(self.writer, "{value}");
    }
}
