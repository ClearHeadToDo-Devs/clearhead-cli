# _C_ommand _L_ine _I_nterface for the _CL_ear _HE_ad framework (cliche)
This is still very experimental, but the ultimate goal is to make an ergonomic, efficient, and powerful command line that will ultimately serve as the root for much of the clearhead ecosystem.

for now the crate is both:
- A cli app (`cliche`)
- And a library (`cliche`)

## Usage
The goal of CLI itself should be relatively straightforward, it will complete CRUD operations on clearhead document(s) using standard configuration practices.

Therefore, subcommands are as follows:
- _C_reate: Create a new action in the document
- _R_ead: Read action(s) on a document
- _U_pdate: Update an action(s) in the document
- _D_elete: Delete an action(s) in the document

In addition, when no subcommand is specified, it will open up the TUI interface to create a keyboard-driven interface that will allow practitioners to interact with the system in a more consistent way. See it as the medium ground between a CLI script and editing the documents directly, heavily inspire by visidata and lazygit.
## Architecture
ultimately, this is going to be the bed of implementing many experimental technologies at once including:
- TreeSitter for using files as both the storage and the interface when necessary
- Persistent data structures to allow for a more data-centric approach ala clojure
- as well as CRDTs for making those immutable data structures easily sharable so one could imagine a collaborative environment where multiple people can update a document, but all versions of the document are still available in history
- and finally, this will generate a set of RDF documents that can be leveraged by other systems using the 5 star framework so that we can begin connecting these documents to the wider web of data, and allow for more advanced operations such as reasoning over the data, or even just querying it in a more structured way.

CRDTs make the collaboration possible, while the persistent data structures will help to keep both the storage size as well as the performance in check while delivering a much simpler interface to implementers

### Values
Experience Rustaceans will likey be disgusted by my code base, I go out of my way to NOT use concrete types, instead opting to convert everything into plain data structures, and use data-based approaches to validation such as json schema, rather than using Rust's type system to enforce the structure of the data.

This has a few important benefits:
- the ultimate goal is to make bindings for other languages to the library, if we make an API consisting on a set of nuanced Rust mechanisms, we are less likely to be able to make those bindings work to other languages
- values have value (pun intended), and the more we can use values to represent the data, the more we can use them in other contexts, such as in a web app or a GUI app
  - specifically, values are *immutable* and we should be create *new* values rather than mutating existing ones, this allows us to have a more predictable and testable code base, as well as a more composable and reusable code base.
- we also want to be considerate citizens of the system. This means that instead of inventing everything single piece from scratch, we try to take advantage of the systems we are running on, taking advantage of native tools where possible and being considerate of the environment we are running in.
  - for example, by using a file as the storage mechanism, we enable people to use whatever editor they want to edit the files directly if they so choose, this is foundational to reducing lock-in and ensuring we are delivering value even if the cli isnt working that day.
- finally, as a functional programmer, I am constantly trying to make apis consisting of pure functions that take values as input and return values as output, rather than having side effects. This allows for a more composable and reusable code base, as well as a more predictable and testable code base.

so there you have it, the two main values (again, pun intended) of this project are:
- *Data-centric*: exchange plain data structures when in a more general context, atleast when it comes to the public API.
  - many of the modules are me taking the stateful Rust APIs and turning them into data-centric APIs, so that they can be used in a more general context.
- *Functional*: use pure functions that take *immutable data* values as input and return *immutable data* values as output, rather than having side effects.
- *Minimal*: Dont reinvent the wheel, take advantage of system tools where possible and enable people to make the workflow that works for them by enabling as many workflows as possible.

however, like clojure, I believe that purity is not the goal, and that we must be pragmatic in our approaches to solving problems, if the only way to achieve something is by using side effects, then so be it, but we should strive to make the side effects as minimal as possible, and aggregate them in a single place, so that we can reason about them more easily.

#### Fitting it all Together
My hope is that previous section gives us a good idea of the direction we are going in, as well as the technologies we are using, but I want to make it clear how each technology has a place within these values

- Using persistent data structures allows us to have a more data-centric approach, as we can represent the data as values rather than as mutable state. We know this because you literally cannot mutate a persistent data structure, you can only create a new one with the changes you want.
  - this has the added benefit of making the borrow checker happy, is much of the problem that rust program run into is that they either:
    - get tangled in a nest of mutable state, hitting some point where they can no longer move forward due to a tangle in the borrow checker
    - or they have to share references to the data, which introduces lifetimes and other complexities that make the code harder to reason about
- Using TreeSitter allows us to take unstructured text (files on the filesystem) and turn them _into_ data.
  - they work wonderfully with persistent data structures, as they also operate with values rather than mutable state, and we can alway reparse the file to get the latest version of the data.
- CRDTs are beautiful because they allow us to represent shared document state as immutable data structures, which means in combination with the persistent data structures, we can have a shared history of the document between several users so we can actually track the history of changes made
- Finally, I feel that much of the problem with other intention managers is that they are too focused on their own ecosystem, and do little to integrate with other pieces of data and applications, when intentions should be the core concept of any sufficiently large... anything! there is always some concept of tasks, actions, or intentions but they need to be reimplemented in every application because nobody is good at integrating data with the wider ecosystem


## Use Case (The why)
I believe that a good command line interface is the foundation of a good ecosystem, as they provide so many benefits for such a low cost:
- scriptability and automation
- composability and interoperability to allow clearhead to be used in a variety of contexts
- a way for people to interact with the system at a user level rather than a developer level
- CLIs can also be the roots of other apps since its not uncommon for a CLI to be the root of a GUI app, or even a web app
- it also gives a good chance to flesh out the library to make sure it can get some exercise before we move into other interface like editor applications or full GUI applications

This will complement the editor plugins by allowing more advanced operations over several documents in a more transactional way, rather than needing to edit the documents directly all the time.


