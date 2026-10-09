# Changelog

## [0.12.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.11.1...state-machines-core-v0.12.0) (2026-10-09)


### Features

* implement Display and Error for every public error type ([027cf53](https://github.com/state-machines/state-machines-rs/commit/027cf53a4c7f83d5a2cccd6a54d64ee00f5de801))

## [0.11.1](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.11.0...state-machines-core-v0.11.1) (2026-10-09)


### Bug Fixes

* **inspect:** render Mermaid edges and validate without per-edge copies ([5094c5e](https://github.com/state-machines/state-machines-rs/commit/5094c5e126b73126296dcf802df5fcd79393be89))

## [0.11.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.10.0...state-machines-core-v0.11.0) (2026-10-03)


### Features

* add bounded eventless transition stabilization ([12e5ab5](https://github.com/state-machines/state-machines-rs/commit/12e5ab503ff7540c2bea762f77f06c4d5215c7f2))
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
* declare native named orthogonal regions and owned routes ([245897b](https://github.com/state-machines/state-machines-rs/commit/245897b89605cc9e996309448f85627ae537ed76))
* declare scope-bound runtime lifecycle work ([0db9e82](https://github.com/state-machines/state-machines-rs/commit/0db9e82654f21aa60dab558530cb1045e9f33b9b))
* distinguish local and external composite transitions ([ee894f7](https://github.com/state-machines/state-machines-rs/commit/ee894f7afa0fe9baacec4f709c486b0ab8ca22ab))
* propagate completion through final composite states ([8192a4c](https://github.com/state-machines/state-machines-rs/commit/8192a4cbcb746d418c955b2f185eb7b2f2a10a15))
* unify owned region snapshots and history restoration ([056543e](https://github.com/state-machines/state-machines-rs/commit/056543e38f6bdd3c7bb93d6d50547c0098ac6f2e))
* validate FSM graphs ([54e023d](https://github.com/state-machines/state-machines-rs/commit/54e023d4bfb16ec82af6e9198da661e5b9ba18be))


### Bug Fixes

* define safe poisoning after interrupted dynamic dispatch ([8456dba](https://github.com/state-machines/state-machines-rs/commit/8456dba25e54687ce6ff664b2db85daf4b22b3e7))

## [0.10.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.9.0...state-machines-core-v0.10.0) (2026-09-14)


### Features

* implement Inspectable, enrich schema, drop dead introspection API ([3b6a2d8](https://github.com/state-machines/state-machines-rs/commit/3b6a2d8adc6a71cce4edfe0fe08f2461a3845df1))

## [0.9.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.8.0...state-machines-core-v0.9.0) (2026-03-22)


### Features

* add fallible async callbacks with rollback semantics ([082611f](https://github.com/state-machines/state-machines-rs/commit/082611f5ba33e668e21725bbf81442b9b4a8a005))

## [0.8.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.7.0...state-machines-core-v0.8.0) (2026-01-17)


### Features

* add introspection and visualization support with Mermaid/JSON export ([#5](https://github.com/state-machines/state-machines-rs/issues/5)) ([2416648](https://github.com/state-machines/state-machines-rs/commit/2416648368151caf8a07955ce96606bc7bf30d73))

## [0.7.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-core-v0.6.0...state-machines-core-v0.7.0) (2025-11-15)


### Features

* add concrete context type support for embedded systems ([003500b](https://github.com/state-machines/state-machines-rs/commit/003500b9b2aeb2204dd7c060d8bbbc6fa0ca81f2))
* add concrete context type support for embedded systems ([0fd546c](https://github.com/state-machines/state-machines-rs/commit/0fd546ccdf45797257fcaa9dfb1c8a47e6659a8e))
* add dynamic dispatch mode for runtime event handling ([4472738](https://github.com/state-machines/state-machines-rs/commit/4472738f84252cb9db69acf68cf527825891765e))
* add state data accessors for dynamic mode (v0.6.0) ([4102b8f](https://github.com/state-machines/state-machines-rs/commit/4102b8f4d8e69e5db03439144de223af0dd94b92))
* enforce snake_case event names with validation ([586fbda](https://github.com/state-machines/state-machines-rs/commit/586fbda2e9808e43b90a753dc192f33b6a82835a))
* implement around callbacks with transaction-like semantics ([f117b83](https://github.com/state-machines/state-machines-rs/commit/f117b83945331c3380f2322f0e70400108a7bd1e))
* implement SubstateOf trait and polymorphic superstate transitions ([0f95e4a](https://github.com/state-machines/state-machines-rs/commit/0f95e4aa85d4423aec8c5b475b095102e70d1e83))
