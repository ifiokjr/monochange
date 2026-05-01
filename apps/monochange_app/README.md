# monochange_app

Release planning toolkit for monorepos — web application.

## Development

```bash
# Start the dev server
cargo leptos watch

# Or with tailwind
npx @tailwindcss/cli -i crates/monochange_app/style/input.css -o crates/monochange_app/style/output.css --watch
cargo leptos watch
```

## Architecture

```
apps/monochange_app/
├── crates/
│   ├── monochange_app/      # Leptos SPA + axum SSR server
│   ├── monochange_app_db/   # Welds ORM models + migrations
│   ├── monochange_app_api/  # OAuth, webhooks, REST handlers
│   └── monochange_app_ai/   # OpenRouter client + AI agents
└── embed/                   # JS feedback widget
```
