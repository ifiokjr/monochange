---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### add JVM ecosystem support (Gradle and Maven)

monochange now discovers and manages JVM projects from Gradle multi-project builds and Maven multi-module projects.

**Configuration:**

```toml
[defaults]
package_type = "jvm"

[package.core]
path = "core"

[package.api]
path = "api"

[ecosystems.jvm]
enabled = true
lockfile_commands = [{ command = "./gradlew dependencies --write-locks" }]
```

**What it discovers:**

- Gradle multi-project builds via `settings.gradle.kts` / `settings.gradle` with `include(...)` directives
- Maven multi-module projects via `pom.xml` with `<modules>` declarations
- Both Kotlin DSL (`build.gradle.kts`) and Groovy DSL (`build.gradle`) build files
- Project versions from `version = "x.y.z"` in Gradle or `<version>` in Maven
- Dependencies from Gradle configurations (`implementation`, `api`, `compileOnly`, `testImplementation`) and Maven scopes (`compile`, `test`, `provided`)

**Version management:**

- Updates `version = "x.y.z"` in `build.gradle.kts` / `build.gradle`
- Updates `<version>` in Maven `pom.xml`
- Updates version entries in Gradle Version Catalogs (`gradle/libs.versions.toml`)
- Skips Maven property references (`${revision}`, `${project.version}`)

**Lockfile commands:**

- Gradle: infers `./gradlew dependencies --write-locks` (prefers wrapper, falls back to bare `gradle`)
- Maven: no inferred default (Maven has no native lockfile)
- Configurable via `[ecosystems.jvm].lockfile_commands`
