---
main: minor
---

#### remove deployments feature

Remove the [[deployments]] configuration section, Deploy CLI step, DeploymentTrigger/DeploymentDefinition/ReleaseDeploymentIntent types, and all related validation, rendering, and documentation. Deployments are a CI concern better handled by native CI workflow triggers.
