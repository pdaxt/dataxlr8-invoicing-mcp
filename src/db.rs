use anyhow::Result;
use sqlx::PgPool;

/// Create the invoicing schema in PostgreSQL if it doesn't exist.
pub async fn setup_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS invoicing;

        CREATE TABLE IF NOT EXISTS invoicing.invoices (
            id              TEXT PRIMARY KEY,
            invoice_number  SERIAL,
            client_name     TEXT NOT NULL,
            client_email    TEXT NOT NULL,
            line_items      JSONB NOT NULL DEFAULT '[]',
            subtotal        DOUBLE PRECISION NOT NULL DEFAULT 0,
            tax             DOUBLE PRECISION NOT NULL DEFAULT 0,
            total           DOUBLE PRECISION NOT NULL DEFAULT 0,
            status          TEXT NOT NULL DEFAULT 'draft'
                            CHECK (status IN ('draft', 'sent', 'paid', 'overdue', 'void')),
            due_date        DATE NOT NULL,
            sent_at         TIMESTAMPTZ,
            paid_at         TIMESTAMPTZ,
            notes           TEXT NOT NULL DEFAULT '',
            created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS invoicing.payments (
            id              TEXT PRIMARY KEY,
            invoice_id      TEXT NOT NULL REFERENCES invoicing.invoices(id) ON DELETE CASCADE,
            amount          DOUBLE PRECISION NOT NULL,
            method          TEXT NOT NULL DEFAULT '',
            reference       TEXT NOT NULL DEFAULT '',
            paid_at         TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoicing.invoices(status);
        CREATE INDEX IF NOT EXISTS idx_invoices_client_email ON invoicing.invoices(client_email);
        CREATE INDEX IF NOT EXISTS idx_invoices_due_date ON invoicing.invoices(due_date);
        CREATE INDEX IF NOT EXISTS idx_payments_invoice_id ON invoicing.payments(invoice_id);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
