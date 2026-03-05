use dataxlr8_mcp_core::mcp::{empty_schema, error_result, get_i64, get_str, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;
const DEFAULT_OFFSET: i64 = 0;

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: String,
    pub invoice_number: i32,
    pub client_name: String,
    pub client_email: String,
    pub line_items: serde_json::Value,
    pub subtotal: f64,
    pub tax: f64,
    pub total: f64,
    pub status: String,
    pub due_date: chrono::NaiveDate,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
    pub notes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Payment {
    pub id: String,
    pub invoice_id: String,
    pub amount: f64,
    pub method: String,
    pub reference: String,
    pub paid_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct InvoiceWithPayments {
    #[serde(flatten)]
    pub invoice: Invoice,
    pub payments: Vec<Payment>,
}

#[derive(Debug, Serialize)]
pub struct InvoiceStats {
    pub total_revenue: f64,
    pub outstanding: f64,
    pub paid_this_month: f64,
    pub invoice_count: i64,
    pub paid_count: i64,
    pub sent_count: i64,
    pub overdue_count: i64,
    pub draft_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PaginatedInvoices {
    pub invoices: Vec<Invoice>,
    pub limit: i64,
    pub offset: i64,
    pub count: usize,
}

// ============================================================================
// Validation helpers
// ============================================================================

/// Extract a string, trim it, and return None if empty after trimming.
fn get_trimmed_str(args: &serde_json::Value, key: &str) -> Option<String> {
    get_str(args, key).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Basic email validation: must contain exactly one '@' with non-empty local and domain parts,
/// and the domain must contain at least one '.'.
fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

/// Validate and clamp pagination parameters.
fn parse_pagination(args: &serde_json::Value) -> (i64, i64) {
    let limit = get_i64(args, "limit")
        .unwrap_or(DEFAULT_LIMIT)
        .max(1)
        .min(MAX_LIMIT);
    let offset = get_i64(args, "offset")
        .unwrap_or(DEFAULT_OFFSET)
        .max(0);
    (limit, offset)
}

const VALID_STATUSES: &[&str] = &["draft", "sent", "paid", "overdue", "void"];

fn is_valid_status(status: &str) -> bool {
    VALID_STATUSES.contains(&status)
}

// ============================================================================
// Tool definitions
// ============================================================================

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_invoice".into(),
            title: None,
            description: Some(
                "Create a new invoice with line items. Automatically calculates subtotal and total."
                    .into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "client_name": { "type": "string", "description": "Client full name" },
                    "client_email": { "type": "string", "description": "Client email address" },
                    "line_items": {
                        "type": "array",
                        "description": "Array of line items: [{description, quantity, unit_price}]",
                        "items": {
                            "type": "object",
                            "properties": {
                                "description": { "type": "string" },
                                "quantity": { "type": "number" },
                                "unit_price": { "type": "number" }
                            },
                            "required": ["description", "quantity", "unit_price"]
                        }
                    },
                    "due_date": { "type": "string", "description": "Due date in YYYY-MM-DD format" },
                    "notes": { "type": "string", "description": "Optional notes for the invoice" },
                    "tax": { "type": "number", "description": "Tax amount (default: 0)" }
                }),
                vec!["client_name", "client_email", "line_items", "due_date"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "get_invoice".into(),
            title: None,
            description: Some("Get a specific invoice by ID, including payment history".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Invoice ID" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "list_invoices".into(),
            title: None,
            description: Some(
                "List invoices with optional filters by status and client. Supports pagination via limit/offset."
                    .into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "status": { "type": "string", "enum": ["draft", "sent", "paid", "overdue", "void"], "description": "Filter by status" },
                    "client": { "type": "string", "description": "Filter by client name or email (partial match)" },
                    "limit": { "type": "integer", "description": "Max results to return (default: 50, max: 500)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default: 0)" }
                }),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "send_invoice".into(),
            title: None,
            description: Some(
                "Mark an invoice as sent and record the sent timestamp".into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Invoice ID to mark as sent" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "record_payment".into(),
            title: None,
            description: Some(
                "Record a payment against an invoice. Marks invoice as paid if fully paid."
                    .into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "invoice_id": { "type": "string", "description": "Invoice ID to record payment for" },
                    "amount": { "type": "number", "description": "Payment amount (must be positive)" },
                    "method": { "type": "string", "description": "Payment method (e.g. bank_transfer, card, cash)" },
                    "reference": { "type": "string", "description": "Payment reference or transaction ID" }
                }),
                vec!["invoice_id", "amount"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "overdue_invoices".into(),
            title: None,
            description: Some(
                "Find all invoices that are past their due date with status 'sent'. Supports pagination via limit/offset."
                    .into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "limit": { "type": "integer", "description": "Max results to return (default: 50, max: 500)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default: 0)" }
                }),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "invoice_stats".into(),
            title: None,
            description: Some(
                "Get invoice statistics: total revenue, outstanding amount, paid this month"
                    .into(),
            ),
            input_schema: empty_schema(),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "void_invoice".into(),
            title: None,
            description: Some("Void/cancel an invoice. Only draft or sent invoices can be voided.".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Invoice ID to void" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
    ]
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct InvoicingMcpServer {
    db: Database,
}

impl InvoicingMcpServer {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // ---- Tool handlers ----

    async fn handle_create_invoice(&self, args: &serde_json::Value) -> CallToolResult {
        let client_name = match get_trimmed_str(args, "client_name") {
            Some(n) => n,
            None => return error_result("Missing or empty required parameter: client_name"),
        };
        let client_email = match get_trimmed_str(args, "client_email") {
            Some(e) => e,
            None => return error_result("Missing or empty required parameter: client_email"),
        };
        if !is_valid_email(&client_email) {
            return error_result("Invalid client_email format. Must be a valid email address (e.g. user@example.com)");
        }
        let line_items = match args.get("line_items") {
            Some(li) => li.clone(),
            None => return error_result("Missing required parameter: line_items"),
        };
        let due_date_str = match get_trimmed_str(args, "due_date") {
            Some(d) => d,
            None => return error_result("Missing or empty required parameter: due_date"),
        };
        let notes = get_trimmed_str(args, "notes").unwrap_or_default();

        // Parse due_date
        let due_date = match chrono::NaiveDate::parse_from_str(&due_date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => return error_result("Invalid due_date format. Use YYYY-MM-DD"),
        };

        // Validate and calculate subtotal from line items
        let items = match line_items.as_array() {
            Some(items) if !items.is_empty() => items,
            Some(_) => return error_result("line_items must be a non-empty array"),
            None => return error_result("line_items must be an array"),
        };

        let mut subtotal: f64 = 0.0;
        for (i, item) in items.iter().enumerate() {
            let desc = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .unwrap_or("");
            if desc.is_empty() {
                return error_result(&format!(
                    "line_items[{i}]: missing or empty 'description'"
                ));
            }
            let qty = match item.get("quantity").and_then(|v| v.as_f64()) {
                Some(q) if q > 0.0 => q,
                Some(_) => {
                    return error_result(&format!(
                        "line_items[{i}]: 'quantity' must be a positive number"
                    ))
                }
                None => {
                    return error_result(&format!(
                        "line_items[{i}]: missing or invalid 'quantity'"
                    ))
                }
            };
            let price = match item.get("unit_price").and_then(|v| v.as_f64()) {
                Some(p) if p >= 0.0 => p,
                Some(_) => {
                    return error_result(&format!(
                        "line_items[{i}]: 'unit_price' must be non-negative"
                    ))
                }
                None => {
                    return error_result(&format!(
                        "line_items[{i}]: missing or invalid 'unit_price'"
                    ))
                }
            };
            subtotal += qty * price;
        }

        let tax = args
            .get("tax")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if tax < 0.0 {
            return error_result("Tax amount must be non-negative");
        }
        let total = subtotal + tax;
        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Invoice>(
            r#"INSERT INTO invoicing.invoices (id, client_name, client_email, line_items, subtotal, tax, total, due_date, notes)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING *"#,
        )
        .bind(&id)
        .bind(&client_name)
        .bind(&client_email)
        .bind(&line_items)
        .bind(subtotal)
        .bind(tax)
        .bind(total)
        .bind(due_date)
        .bind(&notes)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(invoice) => {
                info!(
                    id = id,
                    client = client_name,
                    total = total,
                    "Created invoice"
                );
                json_result(&invoice)
            }
            Err(e) => {
                error!(error = %e, "Failed to create invoice");
                error_result(&format!("Failed to create invoice: {e}"))
            }
        }
    }

    async fn handle_get_invoice(&self, id: &str) -> CallToolResult {
        let invoice: Option<Invoice> = match sqlx::query_as(
            "SELECT * FROM invoicing.invoices WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(i) => i,
            Err(e) => {
                error!(invoice_id = id, error = %e, "Failed to fetch invoice");
                return error_result(&format!("Database error: {e}"));
            }
        };

        match invoice {
            Some(invoice) => {
                let payments: Vec<Payment> = match sqlx::query_as(
                    "SELECT * FROM invoicing.payments WHERE invoice_id = $1 ORDER BY paid_at",
                )
                .bind(id)
                .fetch_all(self.db.pool())
                .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        error!(invoice_id = id, error = %e, "Failed to fetch payments");
                        Vec::new()
                    }
                };

                json_result(&InvoiceWithPayments { invoice, payments })
            }
            None => error_result(&format!("Invoice '{id}' not found")),
        }
    }

    async fn handle_list_invoices(&self, args: &serde_json::Value) -> CallToolResult {
        let status = get_trimmed_str(args, "status");
        let client = get_trimmed_str(args, "client");
        let (limit, offset) = parse_pagination(args);

        // Validate status if provided
        if let Some(ref s) = status {
            if !is_valid_status(s) {
                return error_result(&format!(
                    "Invalid status '{}'. Must be one of: {}",
                    s,
                    VALID_STATUSES.join(", ")
                ));
            }
        }

        // Build query dynamically based on filters
        let mut query = String::from("SELECT * FROM invoicing.invoices WHERE 1=1");
        let mut bind_idx = 1u32;

        if status.is_some() {
            query.push_str(&format!(" AND status = ${bind_idx}"));
            bind_idx += 1;
        }
        if client.is_some() {
            query.push_str(&format!(
                " AND (client_name ILIKE ${bind_idx} OR client_email ILIKE ${bind_idx})"
            ));
            bind_idx += 1;
        }
        query.push_str(&format!(" ORDER BY created_at DESC LIMIT ${bind_idx}"));
        bind_idx += 1;
        query.push_str(&format!(" OFFSET ${bind_idx}"));
        let _ = bind_idx;

        let mut q = sqlx::query_as::<_, Invoice>(&query);

        if let Some(ref s) = status {
            q = q.bind(s);
        }
        if let Some(ref c) = client {
            q = q.bind(format!("%{c}%"));
        }
        q = q.bind(limit);
        q = q.bind(offset);

        match q.fetch_all(self.db.pool()).await {
            Ok(invoices) => {
                let count = invoices.len();
                json_result(&PaginatedInvoices {
                    invoices,
                    limit,
                    offset,
                    count,
                })
            }
            Err(e) => {
                error!(error = %e, "Failed to list invoices");
                error_result(&format!("Database error: {e}"))
            }
        }
    }

    async fn handle_send_invoice(&self, id: &str) -> CallToolResult {
        // Check current status
        let existing: Option<Invoice> = match sqlx::query_as(
            "SELECT * FROM invoicing.invoices WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(i) => i,
            Err(e) => {
                error!(invoice_id = id, error = %e, "Failed to fetch invoice for send");
                return error_result(&format!("Database error: {e}"));
            }
        };

        match existing {
            None => error_result(&format!("Invoice '{id}' not found")),
            Some(inv) if inv.status != "draft" => {
                error_result(&format!(
                    "Cannot send invoice with status '{}'. Only draft invoices can be sent.",
                    inv.status
                ))
            }
            Some(_) => {
                match sqlx::query_as::<_, Invoice>(
                    "UPDATE invoicing.invoices SET status = 'sent', sent_at = now() WHERE id = $1 RETURNING *",
                )
                .bind(id)
                .fetch_one(self.db.pool())
                .await
                {
                    Ok(invoice) => {
                        info!(id = id, "Invoice marked as sent");
                        json_result(&invoice)
                    }
                    Err(e) => {
                        error!(invoice_id = id, error = %e, "Failed to update invoice to sent");
                        error_result(&format!("Failed to send invoice: {e}"))
                    }
                }
            }
        }
    }

    async fn handle_record_payment(&self, args: &serde_json::Value) -> CallToolResult {
        let invoice_id = match get_trimmed_str(args, "invoice_id") {
            Some(id) => id,
            None => return error_result("Missing or empty required parameter: invoice_id"),
        };
        let amount = match args.get("amount").and_then(|v| v.as_f64()) {
            Some(a) if a > 0.0 => a,
            Some(a) if a <= 0.0 => return error_result("Payment amount must be a positive number"),
            _ => return error_result("Missing or invalid required parameter: amount (must be a positive number)"),
        };
        let method = get_trimmed_str(args, "method").unwrap_or_default();
        let reference = get_trimmed_str(args, "reference").unwrap_or_default();

        // Verify invoice exists and is in a payable state
        let invoice: Option<Invoice> = match sqlx::query_as(
            "SELECT * FROM invoicing.invoices WHERE id = $1",
        )
        .bind(&invoice_id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(i) => i,
            Err(e) => {
                error!(invoice_id = %invoice_id, error = %e, "Failed to fetch invoice for payment");
                return error_result(&format!("Database error: {e}"));
            }
        };

        let invoice = match invoice {
            Some(i) => i,
            None => return error_result(&format!("Invoice '{invoice_id}' not found")),
        };

        if invoice.status == "void" {
            return error_result("Cannot record payment on a voided invoice");
        }
        if invoice.status == "paid" {
            return error_result("Invoice is already fully paid");
        }
        if invoice.status == "draft" {
            return error_result("Cannot record payment on a draft invoice. Send it first.");
        }

        let payment_id = uuid::Uuid::new_v4().to_string();

        // Insert payment
        let payment: Payment = match sqlx::query_as(
            r#"INSERT INTO invoicing.payments (id, invoice_id, amount, method, reference)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(&payment_id)
        .bind(&invoice_id)
        .bind(amount)
        .bind(&method)
        .bind(&reference)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(p) => p,
            Err(e) => {
                error!(invoice_id = %invoice_id, error = %e, "Failed to insert payment");
                return error_result(&format!("Failed to record payment: {e}"));
            }
        };

        // Check total payments vs invoice total
        let total_paid: f64 = match sqlx::query_as::<_, (f64,)>(
            "SELECT COALESCE(SUM(amount), 0) FROM invoicing.payments WHERE invoice_id = $1",
        )
        .bind(&invoice_id)
        .fetch_one(self.db.pool())
        .await
        {
            Ok((sum,)) => sum,
            Err(e) => {
                error!(invoice_id = %invoice_id, error = %e, "Failed to sum payments");
                0.0
            }
        };

        // If fully paid, mark invoice as paid
        if total_paid >= invoice.total {
            match sqlx::query(
                "UPDATE invoicing.invoices SET status = 'paid', paid_at = now() WHERE id = $1",
            )
            .bind(&invoice_id)
            .execute(self.db.pool())
            .await
            {
                Ok(_) => info!(id = %invoice_id, "Invoice marked as paid"),
                Err(e) => {
                    error!(invoice_id = %invoice_id, error = %e, "Failed to mark invoice as paid");
                    warn!(invoice_id = %invoice_id, "Payment recorded but invoice status not updated to paid");
                }
            }
        }

        info!(
            invoice_id = %invoice_id,
            amount = amount,
            total_paid = total_paid,
            "Payment recorded"
        );

        json_result(&serde_json::json!({
            "payment": payment,
            "total_paid": total_paid,
            "invoice_total": invoice.total,
            "fully_paid": total_paid >= invoice.total
        }))
    }

    async fn handle_overdue_invoices(&self, args: &serde_json::Value) -> CallToolResult {
        let (limit, offset) = parse_pagination(args);

        match sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoicing.invoices WHERE status = 'sent' AND due_date < CURRENT_DATE ORDER BY due_date ASC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(invoices) => {
                info!(count = invoices.len(), "Found overdue invoices");
                let count = invoices.len();
                json_result(&PaginatedInvoices {
                    invoices,
                    limit,
                    offset,
                    count,
                })
            }
            Err(e) => {
                error!(error = %e, "Failed to fetch overdue invoices");
                error_result(&format!("Database error: {e}"))
            }
        }
    }

    async fn handle_invoice_stats(&self) -> CallToolResult {
        // Total revenue (paid invoices)
        let total_revenue = match sqlx::query_as::<_, (f64,)>(
            "SELECT COALESCE(SUM(total), 0) FROM invoicing.invoices WHERE status = 'paid'",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query total revenue");
                return error_result(&format!("Database error fetching total revenue: {e}"));
            }
        };

        // Outstanding (sent but not paid)
        let outstanding = match sqlx::query_as::<_, (f64,)>(
            "SELECT COALESCE(SUM(total), 0) FROM invoicing.invoices WHERE status IN ('sent', 'overdue')",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query outstanding amount");
                return error_result(&format!("Database error fetching outstanding amount: {e}"));
            }
        };

        // Paid this month
        let paid_this_month = match sqlx::query_as::<_, (f64,)>(
            "SELECT COALESCE(SUM(total), 0) FROM invoicing.invoices WHERE status = 'paid' AND paid_at >= date_trunc('month', CURRENT_DATE)",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query paid this month");
                return error_result(&format!("Database error fetching paid this month: {e}"));
            }
        };

        // Counts by status
        let invoice_count = match sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM invoicing.invoices",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query invoice count");
                return error_result(&format!("Database error fetching invoice count: {e}"));
            }
        };

        let paid_count = match sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM invoicing.invoices WHERE status = 'paid'",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query paid count");
                return error_result(&format!("Database error fetching paid count: {e}"));
            }
        };

        let sent_count = match sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM invoicing.invoices WHERE status = 'sent'",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query sent count");
                return error_result(&format!("Database error fetching sent count: {e}"));
            }
        };

        let overdue_count = match sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM invoicing.invoices WHERE status = 'sent' AND due_date < CURRENT_DATE",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query overdue count");
                return error_result(&format!("Database error fetching overdue count: {e}"));
            }
        };

        let draft_count = match sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM invoicing.invoices WHERE status = 'draft'",
        )
        .fetch_one(self.db.pool())
        .await
        {
            Ok((v,)) => v,
            Err(e) => {
                error!(error = %e, "Failed to query draft count");
                return error_result(&format!("Database error fetching draft count: {e}"));
            }
        };

        let stats = InvoiceStats {
            total_revenue,
            outstanding,
            paid_this_month,
            invoice_count,
            paid_count,
            sent_count,
            overdue_count,
            draft_count,
        };

        json_result(&stats)
    }

    async fn handle_void_invoice(&self, id: &str) -> CallToolResult {
        let existing: Option<Invoice> = match sqlx::query_as(
            "SELECT * FROM invoicing.invoices WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(i) => i,
            Err(e) => {
                error!(invoice_id = id, error = %e, "Failed to fetch invoice for void");
                return error_result(&format!("Database error: {e}"));
            }
        };

        match existing {
            None => error_result(&format!("Invoice '{id}' not found")),
            Some(inv) if inv.status == "paid" => {
                error_result("Cannot void a paid invoice")
            }
            Some(inv) if inv.status == "void" => {
                error_result("Invoice is already voided")
            }
            Some(_) => {
                match sqlx::query_as::<_, Invoice>(
                    "UPDATE invoicing.invoices SET status = 'void' WHERE id = $1 RETURNING *",
                )
                .bind(id)
                .fetch_one(self.db.pool())
                .await
                {
                    Ok(invoice) => {
                        info!(id = id, "Invoice voided");
                        json_result(&invoice)
                    }
                    Err(e) => {
                        error!(invoice_id = id, error = %e, "Failed to void invoice");
                        error_result(&format!("Failed to void invoice: {e}"))
                    }
                }
            }
        }
    }
}

// ============================================================================
// ServerHandler trait implementation
// ============================================================================

impl ServerHandler for InvoicingMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "DataXLR8 Invoicing MCP — create, send, and track invoices with payment recording"
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_
    {
        async {
            Ok(ListToolsResult {
                tools: build_tools(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_
    {
        async move {
            let args =
                serde_json::to_value(&request.arguments).unwrap_or(serde_json::Value::Null);
            let name_str: &str = request.name.as_ref();

            let result = match name_str {
                "create_invoice" => self.handle_create_invoice(&args).await,
                "get_invoice" => match get_trimmed_str(&args, "id") {
                    Some(id) => self.handle_get_invoice(&id).await,
                    None => error_result("Missing or empty required parameter: id"),
                },
                "list_invoices" => self.handle_list_invoices(&args).await,
                "send_invoice" => match get_trimmed_str(&args, "id") {
                    Some(id) => self.handle_send_invoice(&id).await,
                    None => error_result("Missing or empty required parameter: id"),
                },
                "record_payment" => self.handle_record_payment(&args).await,
                "overdue_invoices" => self.handle_overdue_invoices(&args).await,
                "invoice_stats" => self.handle_invoice_stats().await,
                "void_invoice" => match get_trimmed_str(&args, "id") {
                    Some(id) => self.handle_void_invoice(&id).await,
                    None => error_result("Missing or empty required parameter: id"),
                },
                _ => error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
