# User Interface 

The other major Concern of the tool is building the proper user interface for both tools and users. 

As mentioned in the README, we adhere to a verb-noun interface where different commands are structured as `clearhead <verb> <noun> [options]`. This allows us to have a consistent and intuitive command structure that can be easily extended as we add more functionality.

Specifically, the use of subcommands allows for a clear separation of different functionalities and makes it easier for users to discover and use the various features of the tool. For example, we can have subcommands for managing objectives, charters, plans, and acts, each with their own set of options and flags.

## Nouns 

The nouns are mostly standard CRUD operations:
- Create
- Read
- Update
- Delete
- Start 
- Stop

the point is to keep the interface as simple and intuitive as possiible so that users can easily see and compose interactions together

### Read 

Read is where we leverage the oxigraph database to allow users to query that data in a flattened manner without creating a custom query languagec

## Verbs

We use the above file formats as our primary nouns so that each verb can operate on the noun in a consistent way. For example, we can have commands like:

- Objectives
- Charters
- Plans
- Planned Acts
- LSP
- Config

## Flags

The final piece of compisition is the flags which are chared between many of these files although their purpose often changes and certain subcommands have their own specific flags. For example, the `--format` flag is used across many different subcommands to specify the output format, while the `--file` flag is specific to commands that operate on files.

so the common set of flags includes:

- `--format`: Specifies the output format (e.g., JSON, XML, Table)
- `--file`: Specifies the file to operate on (e.g., for create, update
- `--id`: Specifies the ID of the resource to operate on (e.g., for read, update, delete)
- `--help`: Displays help information for the command
- `--verbose`: Enables verbose output for debugging purposes
- `--dry-run`: Simulates the command without making any changes (useful for testing and validation)
- `--where`: Specifies a SPARQL query to filter results (useful for read commands and update/delete commands that operate on multiple resources)

by composing these various verbs, nouns, and flags together we can create a powerful and flexible command-line interface that allows users to easily manage their objectives, charters, plans, and acts while also providing the necessary tools for querying and manipulating the underlying data.

## Examples

Now that we have covered the structure i want to cover how specific functionality is covered without needing to implement a new verb or noun
### Create Items

Any item can be created using simple flags around the noun. For example, to create a new objective, you can use the following command:

```
clearhead_cli create objective "my new objective" --description "this is a description of my new objective" --alias "new-objective"
```

however, we can also create a new objective by reading in a file that contains the objective definition:

```clearhead_cli create objective --file objective.md
```

or even add a list of objectives using a directory of markdown files:

```clearhead_cli create objective --dir objectives/
```
### Start LSP Server

To start the LSP server, you can use the following command:

```
clearhead_cli start lsp
```

### Deleting and Updting Items

to delete an item you can use any of the four reference types (id, file, name, alias) to specify the item you want to delete:

```clearhead_cli delete objective --id 123
clearhead_cli delete objective --file objective.md
clearhead_cli delete objective --name "my new objective"
clearhead_cli delete objective --alias "new-objective"
```

We can also leverage the power of SPARQL queries to delete multiple items at once. For example, to delete all objectives that have a certain tag, you can use the following command:

```
clearhead_cli delete objective --where "{ ?objective a :Objective ; :hasTag :someTag . }"
```

the same can be said for updating items. For example, to update the description of an objective, you can use the following command:

```clearhead_cli update objective --id 123 --description "this is an updated description"
```

or to update multiple items at once using a SPARQL query:


```
clearhead_cli update objective --where "{ ?objective a :Objective ; :hasTag :someTag . }" --description "this is an updated description for all objectives with someTag"
```

### Calendar Events

One core usecase is the creation of calendar events for upcoming planned acts so all we need to do is read all planned acts that have a start time in the future and export them to a calendar format like VEVENT. This can be done with a simple command like:

```
clearhead_cli read acts --where "{ ?act a :PlannedAct ; :startTime ?startTime . FILTER(?startTime > NOW()) }" --format vcalendar
```

By using our simple verb-noun structure and leveraging the power of SPARQL queries, we can easily create complex interactions without needing to add new commands or functionality to the CLI. This allows us to keep the interface simple and intuitive while still providing powerful tools for users to manage their work effectively.

we also provide helper scripts to do these sorts of operations quickly and easily without needing

### Runtime Configuration

Finally, we also want to allow users to configure certain aspects of the CLI at runtime without needing to edit configuration files. For example, we can allow users to set the default output format for all commands using a simple command like:

```
clearhead_cli update config default_format json
```

This would set the default output format to JSON for all commands that support the `--format` flag. We can also allow users to view their current configuration settings with a command like:

```
clearhead_cli read config
```
