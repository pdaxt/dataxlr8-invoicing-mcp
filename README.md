# dataxlr8-invoicing-mcp

Manages invoice creation, tracking, and payment recording for DataXLR8. Handles invoice lifecycle (draft, sent, paid, overdue), line items, tax calculations, and payment history.

## Tools

| Tool | Description |
|------|-------------|
| create_invoice | Create a new invoice with line items. Automatically calculates subtotal and total. |
| get_invoice | Get a specific invoice by ID, including payment history. |
| list_invoices | List invoices with optional filters by status and client. Supports pagination via limit/offset. |
| send_invoice | Mark an invoice as sent and record the sent timestamp. |
| record_payment | Record a payment against an invoice. Marks invoice as paid if fully paid. |
| overdue_invoices | Find all invoices that are past their due date with status 'sent'. Supports pagination via limit/offset. |
| invoice_stats | Get invoice statistics: total revenue, outstanding amount, paid this month. |
| void_invoice | Void/cancel an invoice. Only draft or sent invoices can be voided. |

## Setup

```bash
DATABASE_URL=postgres://dataxlr8:dataxlr8@localhost:5432/dataxlr8 cargo run
```

## Schema

Creates `invoicing` schema in PostgreSQL with tables for invoices and payment tracking.

## Part of

[DataXLR8](https://github.com/pdaxt) - AI-powered recruitment platform
