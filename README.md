<div align="center">

# synthahol-babylon

Library to read presets for the
[Babylon](https://www.waproduction.com/plugins/view/babylon)
synthesizer

[![crates.io][crates.io-badge]][crates.io]
[![Docs][docs-badge]][docs]
[![Workflows][workflows-badge]][workflows]
[![Coverage][coverage-badge]][coverage]
</div>

## Overview

This is a library to read presets for the 
[Babylon](https://www.waproduction.com/plugins/view/babylon)
synthesizer by [W. A. Production](https://www.waproduction.com). 

It was developed independently by Sheldon Young and is not a product of
W. A. Production. Please do not contact them for support.

## Reading and Writing a Preset

```rust
use synthahol_babylon::Preset;

let preset = Preset::read_file("my-preset.bab").unwrap();
```

## Issues

If you have any problems with or questions about this project, please contact
the developers by creating a
[GitHub issue](https://github.com/softdevca/synthahol-babylon/issues).

## Contributing

You are invited to contribute to new features, fixes, or updates, large or
small; we are always thrilled to receive pull requests, and do our best to
process them as fast as we can.

The copyrights of contributions to this project are retained by their
contributors. No copyright assignment is required to contribute to this
project.

## License

Licensed under the Apache License, Version 2.0 (the "License"); you may not use
this file except in compliance with the License. You may obtain a copy of the
License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed
under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
CONDITIONS OF ANY KIND, either express or implied. See the License for the
specific language governing permissions and limitations under the License.

[coverage]: https://coveralls.io/github/maxcountryman/synthahol-babylong?branch=main
[coverage-badge]: https://coveralls.io/repos/github/softdevca/synthahol-babylong/badge.svg?branch=main
[crates.io]: https://crates.io/crates/synthahol-babylon
[crates.io-badge]: https://img.shields.io/crates/v/synthahol-babylon?logo=rust&logoColor=white&style=flat-square
[docs]: https://docs.rs/synthahol-babylon
[docs-badge]: https://docs.rs/synthahol-babylong/badge.svg
[workflows]: https://github.com/softdevca/synthahol-babylong/actions/workflows/rust.yml
[workflows-badge]: https://github.com/softdevca/synthahol-babylong/actions/workflows/rust.yml/badge.svg
