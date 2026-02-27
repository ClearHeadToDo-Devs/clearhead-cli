# architecture overview

Attempts have been made to make the CLI as thin as possible.

We do this by making many of the _core_ functionalities availabile through the `clearhead-core` crate, which is a pure Rust library with no filesystem or CLI dependencies. This allows us to keep the CLI focused on user interaction and file I/O, while the core crate handles all the domain logic, data modeling, and transformations.

This means the cli is largely left to own the UI layer for the terminal, as well as wiring up to the core library in such a way that it can be easily run from either the command line or the LSP server.

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

Everything uses this model, from the CLI to the LSP server, to the CRDT syncing, to the UI rendering. This is the heart of our architecture, and it all relies on these file formats being properly defined and adhered to.

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

## User Interface 

The other major Concern of the tool is building the proper user interface for both tools and users. 

As mentioned in the README, we adhere to a verb-noun interface where different commands are structured as `clearhead <verb> <noun> [options]`. This allows us to have a consistent and intuitive command structure that can be easily extended as we add more functionality.

### Nouns 

The nouns are mostly standard CRUD operations:
- Create
- Read
- Update
- Delete
- Start 
- Stop
- Config

the point is to keep the interface as simple and intuitive as possiible so that users can easily see and compose interactions together

### Verbs

We use the above file formats as our primary nouns so that each verb can operate on the noun in a consistent way. For example, we can have commands like:

- Objectives
- Charters
- Plans
- Planned Acts
- LSP

## Relationship to Library

This is primarily using clearhead-core as the library that outputs all the functions needed to do our work in the commands themselves. while the library is mostly concerned with the domain model and translating work from one format to another, the CLI is responsible for system-level interactions such as:
- Reading and writing files
- Handling user input and output
- Managing the workspace and its structure
- Orchestrating the various commands and their interactions with the core library
- Starting and managing the LSP server for editor integrations
- Config management for runtime configuration




# Reference
[Process Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/process.md
[Naming Conventions]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/naming_conventions.md
[Ontology Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/ontology.md
[Action File Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/action_file_format.md
[Ontology README]: https://github.com/ClearHeadToDo-Devs/ontology/blob/main/README.md
[Objectives Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/objectives.md
[Charter File Specification]: https://github.com/ClearHeadToDo-Devs/specifications/blob/master/charters.md
