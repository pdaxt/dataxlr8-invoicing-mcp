# :receipt: dataxlr8-invoicing-mcp

Invoice lifecycle management for AI agents — create, send, track payments, and monitor overdue invoices.

[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-rmcp_0.17-blue)](https://modelcontextprotocol.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## What It Does

Handles the complete invoice lifecycle from draft to paid through MCP tool calls. Create invoices with line items and automatic totals, mark as sent, record partial or full payments, track overdue invoices, void cancelled work, and pull revenue statistics — all persisted in PostgreSQL with full payment history.

## Architecture

```
                    ┌──────────────────────────┐
AI Agent ──stdio──▶ │  dataxlr8-invoicing-mcp  │
                    │  (rmcp 0.17 server)       │
                    └──────────┬───────────────┘
                               │ sqlx 0.8
                               ▼
                    ┌─────────────────────────┐
                    │  PostgreSQL              │
                    │  schema: invoicing       │
                    │  ├── invoices            │
                    │  ├── line_items          │
                    │  └── payments            │
                    └─────────────────────────┘
```

## Tools

| Tool | Description |
|------|-------------|
| `create_invoice` | Create an invoice with line items, auto-calculates totals |
| `get_invoice` | Get a specific invoice by ID with payment history |
| `list_invoices` | List invoices with optional status and client filters |
| `send_invoice` | Mark an invoice as sent with timestamp |
| `record_payment` | Record a payment, auto-marks as paid when fully settled |
| `overdue_invoices` | Find all invoices past their due date |
| `invoice_stats` | Get total revenue, outstanding amount, and monthly stats |
| `void_invoice` | Void a draft or sent invoice |

## Quick Start

```bash
git clone https://github.com/pdaxt/dataxlr8-invoicing-mcp
cd dataxlr8-invoicing-mcp
cargo build --release

export DATABASE_URL=postgres://user:pass@localhost:5432/dataxlr8
./target/release/dataxlr8-invoicing-mcp
```

The server auto-creates the `invoicing` schema and all tables on first run.

## Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `LOG_LEVEL` | No | Tracing level (default: `info`) |

## Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dataxlr8-invoicing": {
      "command": "./target/release/dataxlr8-invoicing-mcp",
      "env": {
        "DATABASE_URL": "postgres://user:pass@localhost:5432/dataxlr8"
      }
    }
  }
}
```

## Part of DataXLR8

One of 14 Rust MCP servers that form the [DataXLR8](https://github.com/pdaxt) platform — a modular, AI-native business operations suite. Each server owns a single domain, shares a PostgreSQL instance, and communicates over the Model Context Protocol.

## License

MIT
