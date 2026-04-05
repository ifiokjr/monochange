---
monochange: minor
---

#### migrate all variable interpolation to minijinja templates

Replace four separate string interpolation systems with a unified minijinja (Jinja2) template engine. Template syntax changes from {variable} and $variable to {{ variable }}. CLI inputs are now available as template variables in Command steps, supporting conditionals like {% if verbose %}--verbose{% endif %} and array filters like {{ packages | join(",") }}.
