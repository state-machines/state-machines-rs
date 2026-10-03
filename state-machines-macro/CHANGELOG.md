# Changelog

## [0.30.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.22.1...state-machines-macro-v0.30.0) (2026-10-03)


### Features

* add bounded eventless transition stabilization ([12e5ab5](https://github.com/state-machines/state-machines-rs/commit/12e5ab503ff7540c2bea762f77f06c4d5215c7f2))
* add bounded queued deferred and raised event processing ([6da1943](https://github.com/state-machines/state-machines-rs/commit/6da194344a8cc94c6781c981f580542c4f4643c5))
* add child-first hierarchical event selection ([ba8a017](https://github.com/state-machines/state-machines-rs/commit/ba8a017dd2292ee05a977ce73ce19de38c46758d))
* add explicit lifecycle startup and owned entry factories ([8bec661](https://github.com/state-machines/state-machines-rs/commit/8bec66112ed08ce5cbdef29cd97f92341d5de660))
* add final states and completion events ([ad6b6e9](https://github.com/state-machines/state-machines-rs/commit/ad6b6e9edc43a4bfef7e5edb9710b6b8fe8c6044))
* add guarded transition branching ([be96d2d](https://github.com/state-machines/state-machines-rs/commit/be96d2d3cff810cbcf9bed162e9b9ffbecbb7be2))
* add hierarchical lifecycle hooks ([51a248a](https://github.com/state-machines/state-machines-rs/commit/51a248a391accc2eb9a498b667b11095877dd951))
* add internal transitions ([b52e92a](https://github.com/state-machines/state-machines-rs/commit/b52e92a164b667dcd8753d66fcafab7b0b8d019a))
* add owned FSM snapshots ([8b3cb30](https://github.com/state-machines/state-machines-rs/commit/8b3cb30eb4d8fc8984244f0c44ff8880be4e1046))
* add shallow and deep history ([5d86db2](https://github.com/state-machines/state-machines-rs/commit/5d86db217d9a9ba9ea532633f15e620333858909))
* add transition failure hooks ([1a22301](https://github.com/state-machines/state-machines-rs/commit/1a22301f074334279b77a8fbd0f70d32bc07083d))
* advance parent scopes on final-child completion ([a17eae6](https://github.com/state-machines/state-machines-rs/commit/a17eae637a6b00b46216f35fa62d8c11115ba4b6))
* compose orthogonal regions with explicit fork and join ([907b1c5](https://github.com/state-machines/state-machines-rs/commit/907b1c53b44087e15e5b5669ec7007dc7e7f8486))
* declare native named orthogonal regions and owned routes ([245897b](https://github.com/state-machines/state-machines-rs/commit/245897b89605cc9e996309448f85627ae537ed76))
* declare scope-bound runtime lifecycle work ([0db9e82](https://github.com/state-machines/state-machines-rs/commit/0db9e82654f21aa60dab558530cb1045e9f33b9b))
* distinguish local and external composite transitions ([ee894f7](https://github.com/state-machines/state-machines-rs/commit/ee894f7afa0fe9baacec4f709c486b0ab8ca22ab))
* preserve composite-scoped timers and activities across local transitions ([6d3a102](https://github.com/state-machines/state-machines-rs/commit/6d3a102645ce815297aa70cf446ee1c48ebc095f))
* propagate completion through final composite states ([8192a4c](https://github.com/state-machines/state-machines-rs/commit/8192a4cbcb746d418c955b2f185eb7b2f2a10a15))
* **runtime:** add Send runtime ([533792b](https://github.com/state-machines/state-machines-rs/commit/533792bae88fda8bf8fa83a1560ce893fec9b452))
* unify owned region snapshots and history restoration ([056543e](https://github.com/state-machines/state-machines-rs/commit/056543e38f6bdd3c7bb93d6d50547c0098ac6f2e))
* validate FSM graphs ([54e023d](https://github.com/state-machines/state-machines-rs/commit/54e023d4bfb16ec82af6e9198da661e5b9ba18be))


### Bug Fixes

* allow clippy::ptr_arg on generated can_&lt;event&gt; predicates ([eebe4a0](https://github.com/state-machines/state-machines-rs/commit/eebe4a0a594e8e84effee567605e16cef48fd680))
* define safe poisoning after interrupted dynamic dispatch ([8456dba](https://github.com/state-machines/state-machines-rs/commit/8456dba25e54687ce6ff664b2db85daf4b22b3e7))
* make typed state data access fallible ([1acfc80](https://github.com/state-machines/state-machines-rs/commit/1acfc80f321f98aadc1067b239b43a8c30518540))

## [0.22.1](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.22.0...state-machines-macro-v0.22.1) (2026-09-29)


### Bug Fixes

* gate introspection on the linked state-machines inspect feature ([7dae9a4](https://github.com/state-machines/state-machines-rs/commit/7dae9a499fed6121028d7ebe4a29bb0d893a1256))

## [0.22.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.21.0...state-machines-macro-v0.22.0) (2026-09-14)


### Features

* generate can_&lt;event&gt; predicates in typestate mode ([38ae2d4](https://github.com/state-machines/state-machines-rs/commit/38ae2d48ca67a85cc41d0eda251632d8c5f7e615))
* implement global filtered callbacks block ([e7e4707](https://github.com/state-machines/state-machines-rs/commit/e7e4707ad135886a323e0899e4b4f2ff61cdebca))
* implement Inspectable, enrich schema, drop dead introspection API ([3b6a2d8](https://github.com/state-machines/state-machines-rs/commit/3b6a2d8adc6a71cce4edfe0fe08f2461a3845df1))
* implement superstate data lifecycle in typestate mode ([c69006b](https://github.com/state-machines/state-machines-rs/commit/c69006b5c71469f8164741b43583e71fd91d2edf))


### Bug Fixes

* remove dead superstate blanket-impl codegen path ([6a744f9](https://github.com/state-machines/state-machines-rs/commit/6a744f9ca497e45ac2ece9b8ba664067020fc928))

## [0.21.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.20.0...state-machines-macro-v0.21.0) (2026-06-27)


### Features

* add is_available_event to dynamic machines ([5547b26](https://github.com/state-machines/state-machines-rs/commit/5547b26896109cbe1e1d61979ab4e016942da694))

## [0.20.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.9.0...state-machines-macro-v0.20.0) (2026-06-15)


### ⚠ BREAKING CHANGES

* `current_state()` returns the generated state enum instead of `&'static str`. Match on `{Machine}State` variants, or call `.name()` / `.to_string()` for the previous string form.

### Features

* extract shared macro codegen and parser helpers ([cb8a98a](https://github.com/state-machines/state-machines-rs/commit/cb8a98a0b795f26dc4bf5b71b2c0ad2832840322))
* typed runtime state enum for dynamic machines ([#12](https://github.com/state-machines/state-machines-rs/issues/12)) ([6f90319](https://github.com/state-machines/state-machines-rs/commit/6f903194d315679a1210b7907c97f7a437b7f0e6))

## [0.9.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.8.1...state-machines-macro-v0.9.0) (2026-03-22)


### Features

* add fallible async callbacks with rollback semantics ([082611f](https://github.com/state-machines/state-machines-rs/commit/082611f5ba33e668e21725bbf81442b9b4a8a005))

## [0.8.1](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.8.0...state-machines-macro-v0.8.1) (2026-01-23)


### Bug Fixes

* clippy error ([5e61436](https://github.com/state-machines/state-machines-rs/commit/5e61436e6e8af9c96ace8a544e1a122de2281dab))
* inspect is optional feaature ([ed4c072](https://github.com/state-machines/state-machines-rs/commit/ed4c072243864f0602ee4df6271fd5fcdc700e89))

## [0.8.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.7.1...state-machines-macro-v0.8.0) (2026-01-17)


### Features

* add introspection and visualization support with Mermaid/JSON export ([#5](https://github.com/state-machines/state-machines-rs/issues/5)) ([2416648](https://github.com/state-machines/state-machines-rs/commit/2416648368151caf8a07955ce96606bc7bf30d73))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * state-machines-core bumped from 0.7.0 to 0.8.0

## [0.7.1](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.7.0...state-machines-macro-v0.7.1) (2025-11-15)


### Bug Fixes

* enable cargo-workspace plugin and update internal deps to 0.7.0 ([296c963](https://github.com/state-machines/state-machines-rs/commit/296c963bb826cdc1b818edd453d31758dcf7a1fc))

## [0.7.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-macro-v0.6.0...state-machines-macro-v0.7.0) (2025-11-15)


### Features

* add concrete context type support for embedded systems ([003500b](https://github.com/state-machines/state-machines-rs/commit/003500b9b2aeb2204dd7c060d8bbbc6fa0ca81f2))
* add concrete context type support for embedded systems ([0fd546c](https://github.com/state-machines/state-machines-rs/commit/0fd546ccdf45797257fcaa9dfb1c8a47e6659a8e))
* add dynamic dispatch mode for runtime event handling ([4472738](https://github.com/state-machines/state-machines-rs/commit/4472738f84252cb9db69acf68cf527825891765e))
* add state data accessors for dynamic mode (v0.6.0) ([4102b8f](https://github.com/state-machines/state-machines-rs/commit/4102b8f4d8e69e5db03439144de223af0dd94b92))
* add state-local storage accessors for hierarchical states ([4d24314](https://github.com/state-machines/state-machines-rs/commit/4d243147771dd62b089be5a62b94deed81a49733))
* add state-specific data accessors and automatic storage lifecycle ([d64f84f](https://github.com/state-machines/state-machines-rs/commit/d64f84fcefbd0cde802fab9352fe60e1a0fff813))
* enforce snake_case event names with validation ([586fbda](https://github.com/state-machines/state-machines-rs/commit/586fbda2e9808e43b90a753dc192f33b6a82835a))
* implement around callbacks with transaction-like semantics ([f117b83](https://github.com/state-machines/state-machines-rs/commit/f117b83945331c3380f2322f0e70400108a7bd1e))
* implement SubstateOf trait and polymorphic superstate transitions ([0f95e4a](https://github.com/state-machines/state-machines-rs/commit/0f95e4aa85d4423aec8c5b475b095102e70d1e83))
* update criterioni package ([9d6d932](https://github.com/state-machines/state-machines-rs/commit/9d6d932fa5fc0ad4e623873fb062c7135a5b8837))


### Bug Fixes

* generate superstate markers and avoid duplicate data() methods ([b0838f0](https://github.com/state-machines/state-machines-rs/commit/b0838f0312d16ff53939eaccaa8f4c8813317436))
* storage rollback corruption and clippy compliance ([4e5a5fa](https://github.com/state-machines/state-machines-rs/commit/4e5a5fa980a0b2f7ebb773d7808040ce38b1180a))
* suppress naming convention warnings in dynamic_dispatch test ([0b5cfa9](https://github.com/state-machines/state-machines-rs/commit/0b5cfa99bed7f086d1de0a962b8ae10fddd38a30))
