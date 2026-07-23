# architecture overview

Attempts have been made to make the CLI as thin as possible.

We do this by making many of the _core_ functionalities availabile through the `clearhead-core` crate, which is a pure Rust library with no filesystem or CLI dependencies. This allows us to keep the CLI focused on user interaction and file I/O, while the core crate handles all the domain logic, data modeling, and transformations.

The CLI owns the terminal UI and synchronous command workflows over core. The separate `clearhead-lsp` process also depends directly on core for editor protocol workflows; it does not route through this crate.

## Conceptual Model

The **Rust struct (IR) is the canonical representation**. Everything else is a view or persistence mechanism. The IR aligns with the [Actions Ontology](../ontology/README.md), specifically the ActionPlan/ActionProcess distinction from BFO/CCO.

```
                         IR (Rust Structs)
                    (canonical in-memory type)
                    Objectives, Charters, Plans, Acts
                              │
                    ┌─────────┴─────────┐
                    │                   │
                    ▼                   ▼
               Workspace            Oxigraph
              (text source          (query cache
               for editors)          for SPARQL)
```

- Workspace is all the plaintext files that users interact with directly. These files are the durable source of truth and are parsed into the IR.
- **Oxigraph** is an ephemeral query cache materialized from the IR. Enables SPARQL queries and SHACL validation.
- Distributed synchronization is deferred to a future integration and is not part of the CLI or core build.

## Workspace Architecture

Per the [Process Specification][Process Specification], cli leverages our [Naming Conventions][Naming Conventions] to actually discover and build the proper domain model.

This is really where the rubber hits the road between the [Ontology Domain][Ontology Specification] and the various specs that we have defined for finding those file formats

### File Format Distinctions

In particular, its important to know how the different domain models translate to different files within the workspace:
- `Objectives` -> `.md` files within the `objectives/` directory per our and following the [Objectives File Specification][Objectives File Specification]
- `Charters` -> `.md` files within the workspace root or any subdirectory and is written according to the [Charter File Specification][Charter File Specification]
- `Plans` -> `.actions` files that conform to the [Action File Specification][Action File Specification]
- `Planned Acts` -> `.ttl` files that conform to the [defined ontology][Ontology README]

These four file formats come together to allow us to form and update the `DomainModel` in memory, which will enable the core of what we are doing here

Everything uses this model, from the CLI to the LSP server and UI rendering. This is the heart of our architecture, and it all relies on these file formats being properly defined and adhered to.

As things get closed, we move them to a final `archive.ttl` file that serves as a historical record of all completed objectives, charters, plans, and acts. This file is also in TTL format and conforms to the same ontology as the planned acts.

With this, we are able to start doing SPARQL queries against the whole history of the domain despite only capturing workin plaintext files

#### File Conversions

What needs to be known is that converting between different file formats is a core part of the architecture and we are intending to participate in delivering functionality by supporting the following conversions:
- Plan DSL (Action files)
- Mardown (Objectives and Charters)
- TTL (Planned Acts)
- JSON
- VEVENT (For calendar integrations)

By getting these structures in place we can easily deliver functionality by simply making different structures available in different formats

#### Calendar Export Boundary

When plans are exported as `.ics` files, the CLI writes them to:

```
$XDG_DATA_HOME/clearhead/plans/<charter-slug>/<plan-uid>.ics
```

This is an **output boundary** — the CLI's responsibility ends at writing a valid iCalendar file to that path. Sync to external calendar systems (Google Calendar, CalDAV servers, etc.) is handled entirely by external tooling (e.g., vdirsyncer). The CLI has no dependency on any sync tool and makes no assumptions about what, if anything, consumes these files.

This is intentional. Keeping sync out of the CLI means:
- the CLI remains testable without network or credentials
- operators choose their own sync strategy
- the `.ics` files are usable standalone by any CalDAV-aware tool

Project-local workspaces (those with a `.clearhead/` directory at the project root) write plans to `.clearhead/plans/` within that project. These are development workspace files and are not expected to be in the personal calendar sync path.


## Relationship to Library

This is primarily using clearhead-core as the library that outputs all the functions needed to do our work in the commands themselves. while the library is mostly concerned with the domain model and translating work from one format to another, the CLI is responsible for system-level interactions such as:
- Reading and writing files
- Handling user input and output
- Managing the workspace and its structure
- Orchestrating the various commands and their interactions with the core library
- Providing a temporary external `clearhead start lsp` compatibility shim
- Config management for runtime configuration


# Reference
[Process Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/process.md
[Naming Conventions]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/naming_conventions.md
[Ontology Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/ontology.md
[Action File Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/action_file_format.md
[Ontology README]: https://github.com/ClearHeadToDo-Devs/ontology/blob/main/README.md
[Objectives Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/objectives.md
[Charter File Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/charters.md
