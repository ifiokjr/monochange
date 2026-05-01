// monochange Feedback Widget
// Embed this in your app, website, or terminal to collect user feedback.

interface WidgetConfig {
  formId: string;
  repo: string;
  theme?: "light" | "dark";
  position?: "bottom-right" | "bottom-left";
}

class MonochangeWidget {
  private config: WidgetConfig;

  constructor(config: WidgetConfig) {
    this.config = config;
  }

  mount(_container: HTMLElement): void {
    // TODO: Implement widget UI
  }

  show(): void {
    // TODO: Show widget
  }

  hide(): void {
    // TODO: Hide widget
  }
}

// Export for direct usage
export { MonochangeWidget, type WidgetConfig };

// Auto-init from script tag
const script = document.currentScript as HTMLScriptElement | null;
if (script?.dataset.monochangeRepo) {
  const widget = new MonochangeWidget({
    formId: script.dataset.monochangeFormId ?? "",
    repo: script.dataset.monochangeRepo,
    theme: (script.dataset.monochangeTheme as "light" | "dark") ?? "light",
  });
  const container = document.createElement("div");
  container.id = "monochange-widget";
  document.body.append(container);
  widget.mount(container);
}
