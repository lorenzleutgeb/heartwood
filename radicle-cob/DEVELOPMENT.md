# Implementing a new COB

* There is little to help in `heartwood` for implementing a COB yet.
  This may change. At the moment it's probably best to start with an
  existing COB, make a copy, and change it as needed.
* The `CollaborativeObject` in `radicle-cob` is the implementation of
  the generic aspects of a COB. For each type of COB, a specific
  wrapper needs to be implemented. The wrapper provides a Rust type
  and implement type safe operations on the COB.
  - Another way of thinking about this is that you create a domain
    type that is a materialised view of the `CollaborativeObject`.
  - Overall, the approach is that the generic COB implementation
    represents a COB as a series of changes from a start value, and
    the specific COB implementation produces a materialised view by
    applying the changes in order.
* Choose a name for the COB. In this document we use the name `Corn`.
* You need to create a new module:
  - `radicle/src/cob/corn.rs`
    - this is the actual COB type that wraps the generic COB
    - add `pub mod corn;` to `radicle/src/cob.rs`
* You probably want to add commands to the command line to let the
  user interact with or manipulate the COBs. This is not covered by
  this document, for brevity.
  - `radicle-cli/src/commands/corn.rs`
    - this implements subcommands for manipulate the new COB type
  - `radicle-cli/src/terminal/corn.rs`
    - this implements displaying the COB on the terminal
* In `radicle/src/cob/corn.rs` you need to do the following:
  - Choose a textual name for the COB type, e.g., `com.example.corn`.
    This is needed to distinguish the COB from other COBs. You need to
    use a reverse domain name notation, using a domain whose owners
    are OK with you using the name, to avoid naming conflicts. For
    example, Radicle issues use the name `xyz.radicle.issue`
    (`radicle.xyz` being the Radicle project domain name).
  
    ~~~rust
    use once_cell::sync::Lazy;
    use crate::cob::TypeName;

    pub static TYPENAME: Lazy<TypeName> =
        Lazy::new(|| FromStr::from_str("com.example.corn").expect("type name is valid"));
    ~~~

  - each COB needs an ID and there should be a type or type alias for that.

    ~~~rust
    use crate::cob::ObjectId;
 
    pub type BuildId = ObjectId;
    ~~~

  - most COBs have a state the can be updated when the COB is
    changed: think issue being open or closed, for example

    ~~~rust
    #[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", tag = "status")]
    pub enum State {
        /// COB is fresh.
        #[default]
        Fresh,
        /// COB is roasted.
        Roasted,
        /// COB is buttered,
        Buttered,
    }
    ~~~

  - define all the ways to update the COB that are allowed; these are
    called actions (or sometimes operations)

    ~~~rust
    #[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum Action {
        LifeCycle { state: State },
        ....
    }

    impl CobAction for Action {}
    pub type Op = cob::Op<Action>;
    ~~~

  - define the Rust type for the processed COB, with all the fields
    that the COB stores; this is not stored in the repository
    (`radicle-cob` does that as a sequence of operations), but it's
    used to represent the the COB in memory after all the actions have
    been taken

    ~~~rust
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Corn {
        state: State,
        ...
    }

    impl Corn {
        pub fn state(&self) -> State {
            self.state
        }
    }

    /// Apply a single action to the build.
    fn action<R: ReadRepository>(
        &mut self,
        action: Action,
        ...
    ) -> Result<(), Error> {
        match action {
            Action::Lifecycle { state } => { self.state = state },
            ...
        }
        Ok(())
    }
    ~~~

  - implement the `radicle_cob::Cob` trait for your COB: this allows
    the generic COB mechanism to store and retrieve the COB

    ~~~rust
    impl store::Cob for Corn {
        type Action = Action;
        type Error = Error;

        fn type_name() -> &'static TypeName {
            &TYPENAME
        }

        fn from_root<R: ReadRepository>(op: Op, repo: &R) -> Result<Self, Self::Error> {
            let actions = op.actions.into_iter();
            let mut corn = Corn::default();

            for action in actions {
                corn.action(action, op.id, op.author, op.timestamp, op.identity, repo)?;
            }
            Ok(build)
        }

        fn op<R: ReadRepository>(&mut self, op: Op, repo: &R) -> Result<(), Error> {
            ...
            Ok(())
        }
    }

    impl<R: ReadRepository> cob::Evaluate<R> for Corn {
        type Error = Error;

        fn init(entry: &cob::Entry, repo: &R) -> Result<Self, Self::Error> {
            let op = Op::try_from(entry)?;
            let object = Corn::from_root(op, repo)?;
            Ok(object)
        }

        fn apply(&mut self, entry: &cob::Entry, repo: &R) -> Result<(), Self::Error> {
            let op = Op::try_from(entry)?;
            self.op(op, repo)
        }
    }

    impl<R: ReadRepository> store::Transaction<Corn, R> {
        pub fn lifecycle(&mut self, state: State) -> Result<(), store::Error> {
            self.push(Action::Lifecycle { state })
        }
    }
    ~~~

  - implement a mutable version of the COB Rust type: we can't modify
    the base COB type in Rust directly, as all changes need to happens
    via the `radicle-cob` mechanism

    ~~~rust
    impl<'a, 'g, R> From<CornMut<'a, 'g, R>> for (CornId, Corn) {
        fn from(value: CornMut<'a, 'g, R>) -> Self {
            (value.id, value.corn)
        }
    }

    pub struct CornMut<'a, 'g, R> {
        id: ObjectId,
        oorn: Corn,
        store: &'g mut Corns<'a, R>,
    }

    impl<'a, 'g, R> std::fmt::Debug for CornMut<'a, 'g, R> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("CornMut")
                .field("id", &self.id)
                .field("corn", &self.corn)
                .finish()
        }
    }

    impl<'a, 'g, R> CornMut<'a, 'g, R>
    where
        R: WriteRepository + cob::Store,
    {
        /// Reload the build data from storage.
        pub fn reload(&mut self) -> Result<(), store::Error> {
            self.corn = self
                .store
                .get(&self.id)?
                .ok_or_else(|| store::Error::NotFound(TYPENAME.clone(), self.id))?;

            Ok(())
        }

        pub fn id(&self) -> &ObjectId {
            &self.id
        }

        pub fn lifecycle<G: Signer>(&mut self, state: State, signer: &G) -> Result<EntryId, Error> {
            self.transaction("Lifecycle", signer, |tx| tx.lifecycle(state))
        }

        pub fn transaction<G, F>(
            &mut self,
            message: &str,
            signer: &G,
            operations: F,
        ) -> Result<EntryId, Error>
        where
            G: Signer,
            F: FnOnce(&mut Transaction<Build, R>) -> Result<(), store::Error>,
        {
            let mut tx = Transaction::default();
            operations(&mut tx)?;

            // Apply the chanes to the COB here.
            let (corn, something) = tx.commit(message, self.id, &mut self.store.raw, signer)?;
            self.corn = corn;

            Ok(something)
        }
    }

    impl<'a, 'g, R> Deref for CornMut<'a, 'g, R> {
        type Target = Corn;

        fn deref(&self) -> &Self::Target {
            &self.corn
        }
    }
    ~~~

  - implement a way to access the new COB in the Radicle repository
    using the new native Rust type
  
    ~~~rust
    pub struct Corns<'a, R> {
        raw: store::Store<'a, Corn, R>,
    }

    impl<'a, R> Deref for Corns<'a, R> {
        type Target = store::Store<'a, Corn, R>;

        fn deref(&self) -> &Self::Target {
            &self.raw
        }
    }

    impl<'a, R: WriteRepository> Corns<'a, R>
    where
        R: ReadRepository + cob::Store,
    {
        pub fn open(repository: &'a R) -> Result<Self, store::Error> {
            let raw = store::Store::open(repository)?;
            Ok(Self { raw })
        }

        pub fn get(&self, id: &ObjectId) -> Result<Option<Corn>, store::Error> {
            self.raw.get(id)
        }

        pub fn get_mut<'g>(&'g mut self, id: &ObjectId) -> Result<CornMut<'a, 'g, R>, store::Error> {
            let build = self
                .raw
                .get(id)?
                .ok_or_else(move || store::Error::NotFound(TYPENAME.clone(), *id))?;

            Ok(BuildMut {
                id: *id,
                build,
                store: self,
            })
        }

        pub fn create<'g, G: Signer>(
            &'g mut self,
            value: Type, // this depends on the COB
            signer: &G,
        ) -> Result<CornMut<'a, 'g, R>, Error> {
            let (id, corn) = Transaction::initial("Create corn", &mut self.raw, signer, |tx| {
                tx.set_field(value)?;
                Ok(())
            })?;

            Ok(CornMut {
                id,
                corn,
                store: self,
            })
        }

        pub fn remove<G: Signer>(&self, id: &ObjectId, signer: &G) -> Result<(), store::Error> {
            self.raw.remove(id, signer)
        }
    }
    ~~~

  - if you need to set some fields to values at creation time, and
    prevent those from being modified later, the pattern to implement
    this to have an action to set the value, and make sure it's the
    first action when re-constructing a COB, and it to be an error if
    is happens later
