# AGENTS.md Structure Reference

Detailed template and guidance for each section of an AGENTS.md file.
Include only the sections relevant to the project — not all are required.

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Commands](#2-commands)
3. [Project Structure](#3-project-structure)
4. [Technical Stack](#4-technical-stack)
5. [Code Style & Conventions](#5-code-style--conventions)
6. [Testing](#6-testing)
7. [Git Workflow](#7-git-workflow)
8. [Boundaries](#8-boundaries)
9. [References](#9-references)

---

## 1. Project Overview

A brief description of what the project does and its purpose. Keep it to 2-3 sentences.
If a README already covers this well, a single sentence plus a link is enough.

```markdown
# My Service

Backend API for managing customer workflows. Built with Kotlin and Spring Boot.

For full documentation see [README.md](README.md).
```

## 2. Commands

Place these early — agents reference them frequently. Include the exact commands with flags.

```markdown
## Commands

- **Build**: `./gradlew build`
- **Test (unit)**: `./gradlew test`
- **Test (integration)**: `./gradlew integrationTest`
- **Lint**: `./gradlew ktlintFormat`
- **Run locally**: `./gradlew bootRun --args='--spring.profiles.active=local'`
```

Be precise with flags and profiles. If there are environment prerequisites (Docker, env vars), state them.

## 3. Project Structure

Map the key directories so agents know where to find and place code.
Focus on what's non-obvious — skip standard framework layouts.

```markdown
## Project Structure

- `src/main/kotlin/com/example/` — Application code
  - `api/` — REST controllers and DTOs
  - `domain/` — Business logic and domain models
  - `infrastructure/` — Database, external service clients
- `src/test/` — Unit tests (mirrors main structure)
- `src/integrationTest/` — Integration tests with test containers
- `config/` — Environment-specific configuration
```

## 4. Technical Stack

Be specific about versions and key dependencies. This prevents agents from suggesting
incompatible libraries or outdated patterns.

```markdown
## Technical Stack

- Kotlin 1.9, Java 21
- Spring Boot 3.2 with WebFlux (reactive)
- Gradle 8.5 with Kotlin DSL
- MongoDB with Spring Data Reactive
- Temporal for workflow orchestration
- Jackson for JSON serialization
```

## 5. Code Style & Conventions

Show concrete examples of the project's patterns. One example is worth more than a paragraph of rules.

```markdown
## Code Style

### Naming
- Classes: PascalCase (`CustomerService`)
- Functions: camelCase (`findCustomerById`)
- Constants: SCREAMING_SNAKE_CASE in companion objects

### Error Handling
We use sealed classes for domain errors:

​```kotlin
sealed class DomainError {
    data class NotFound(val id: String) : DomainError()
    data class ValidationFailed(val reasons: List<String>) : DomainError()
}
​```

### API Response Pattern
All API endpoints return a standard envelope:

​```kotlin
data class ApiResponse<T>(
    val data: T?,
    val errors: List<ApiError> = emptyList()
)
​```
```

Prioritize patterns that agents frequently get wrong or that differ from common defaults.

## 6. Testing

Specify frameworks, conventions, and expectations so agents write tests that match the project.

```markdown
## Testing

- **Unit tests**: JUnit 5 + MockK. Place in `src/test/` mirroring the main source structure.
- **Integration tests**: Use `@SpringBootTest` with Testcontainers. Place in `src/integrationTest/`.
- **Coverage**: Minimum 80% enforced by Kover. Check with `./gradlew koverVerify`.
- **Naming**: `should <expected behavior> when <condition>` (e.g., `should return 404 when customer not found`)

### Test example

​```kotlin
@Test
fun `should return customer when valid id is provided`() {
    val customer = Customer(id = "123", name = "Test")
    every { repository.findById("123") } returns customer

    val result = service.findById("123")

    result shouldBe customer
    verify(exactly = 1) { repository.findById("123") }
}
​```
```

## 7. Git Workflow

Clarify branch naming, commit message format, and PR expectations.

```markdown
## Git Workflow

- **Branch naming**: `feature/<ticket-id>-short-description`, `fix/<ticket-id>-short-description`
- **Commit messages**: Conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`)
- **PRs**: Target `main`. Include a summary and test plan.
- Run `./gradlew ktlintFormat` before committing.
```

## 8. Boundaries

Use the three-tier system to set clear expectations.

```markdown
## Boundaries

### Always do
- Run `./gradlew ktlintFormat` before suggesting code changes
- Use existing domain patterns and error types
- Write tests for new functionality
- Use constants from companion objects instead of hardcoded strings

### Ask first
- Adding new dependencies to `build.gradle.kts`
- Modifying public API contracts
- Changing database schemas or migrations
- Deleting files or removing functionality

### Never do
- Commit secrets, API keys, or credentials
- Modify CI/CD pipeline files without explicit request
- Skip or delete failing tests
- Use `var` when `val` works
- Introduce new frameworks or libraries without discussion
```

## 9. References

When the AGENTS.md links to reference files in `.agents/references/`, explain when to consult each one.

```markdown
## References

For detailed documentation, see `.agents/references/`:

- **[architecture.md](.agents/references/architecture.md)** — Read when working on cross-module features or service boundaries
- **[api-patterns.md](.agents/references/api-patterns.md)** — Read when creating or modifying API endpoints
- **[data-model.md](.agents/references/data-model.md)** — Read when working with database entities or queries
```
