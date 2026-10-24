# Universe factories

Normal factories plan one implicit environment. Universe factories plan several
locations together and are intended for cross-planet production chains.

## Data model

- `Location = Planet(name) | Space`; a planet's name is also its surface name
  (the current game data does not support multiple surfaces per planet).
- `LocatedDualVar { location, flow }` is the LP coordinate for one flow at one
  location.
- `LocatedMechanic::Local { location, mechanic }` wraps an existing
  `Mechanic` at a location. The local mechanic is expanded with that location's
  environment.
- `LocatedMechanic::RocketExport { location, item }` consumes one item at a planet
  and its resolved item weight in `RocketWeightCapacity`, then creates the same
  item at `Space`, at zero cost.
- `LocatedMechanic::OrbitalDrop { location, item }` consumes one Space item and
  delivers it to the destination planet. Cost is `1 / stack_size` per item; both
  directions preserve quality. Neither mechanism accepts Space as `location`.

## Balance and conversion

Each location has its own flow balance. Planet implicit sources come from the
same resource computation as normal factories; Space sources are all asteroid
chunk items. Space items reach planets only through explicit OrbitalDrop mechanics.
RocketExport requires a Rocket Silo recipe to produce
`RocketWeightCapacity` at the source planet.

## UI

The project header lists universe factories next to normal factories. `✦`
creates one; selecting it opens the universe workspace. Environment and strict
constraints are not edited here: every local mechanic owns its location's
environment. The workspace shares the factory workflow where it applies:

- list/grid views and text filtering for located mechanics;
- target editing and arbitrary linear target expressions: every term carries
  its own location, so one expression can mix items from any planets or Space;
- local mechanic editing (recipe/mining/item-fuel inline, advanced JSON for the
  remaining kinds), per-mechanic location editing, rocket exports and orbital drops,
  enable/disable/delete;
- global automatic planning: enumerate every planet's local candidates, all
  Space candidates, and both cargo directions for every planet and item quality, then solve one
  joint universe LP;
- solve results grouped by location; optional location clustering in both list and
  network views. Clustering never changes document order or solver inputs.

## Regression checks

`tests/orbital_cargo.rs` verifies direction, quality, weight consumption, stack cost,
missing/disabled drops, joint planning/writeback and editable undoable transport entries.
It selects SoPlex when compiled with that backend:

```powershell
cargo test -p metatorio-runtime --no-default-features --features microlp,rust-lp,soplex --test orbital_cargo
```

Frontend tests cover the explicit graph edges, real ELK location layouts, both add
buttons, list/network grouping, ordinary-factory isolation and virtual scrolling.
The unpublished universe format uses only `rocket-export` and `orbital-drop`; the
old `transfer` tag and command names are intentionally not accepted.
