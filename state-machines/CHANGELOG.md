# Changelog

## [0.7.0](https://github.com/state-machines/state-machines-rs/compare/state-machines-v0.6.0...state-machines-v0.7.0) (2025-11-15)


### Features

* add comprehensive performance benchmarks ([1f984d3](https://github.com/state-machines/state-machines-rs/commit/1f984d3b683609f322bc5f92d72f0f555aeabd88))
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

* add local README copy for packaging ([a764fc1](https://github.com/state-machines/state-machines-rs/commit/a764fc1b31e4aecb53442a55ae39011ec197ec50))
* correct README path for cargo publish ([0cc66ed](https://github.com/state-machines/state-machines-rs/commit/0cc66ed46737b0ea4aa72b3767743350f4463e15))
* correct README path in package and remove dead doc links ([50d45c6](https://github.com/state-machines/state-machines-rs/commit/50d45c681ced9ee0d78f03af1e5d1053cf42f56c))
* correct README.md path in package include ([6d62778](https://github.com/state-machines/state-machines-rs/commit/6d627786d832678a41c03265a6f84eb2e20fd0ba))
* generate superstate markers and avoid duplicate data() methods ([b0838f0](https://github.com/state-machines/state-machines-rs/commit/b0838f0312d16ff53939eaccaa8f4c8813317436))
* storage rollback corruption and clippy compliance ([4e5a5fa](https://github.com/state-machines/state-machines-rs/commit/4e5a5fa980a0b2f7ebb773d7808040ce38b1180a))
* suppress lint warnings in tests and benchmarks ([292b2e2](https://github.com/state-machines/state-machines-rs/commit/292b2e28135308628becd255b3a1ccef5fd98049))
* suppress naming convention warnings in dynamic_dispatch test ([0b5cfa9](https://github.com/state-machines/state-machines-rs/commit/0b5cfa99bed7f086d1de0a962b8ae10fddd38a30))
