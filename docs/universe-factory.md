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
- `LocatedMechanic::Transfer { location, item }` consumes one item at a planet
  and its resolved item weight in `RocketWeightCapacity`, then creates the same
  item at `Space`.

## Balance and conversion

Each location has its own flow balance. Planet implicit sources come from the
same resource computation as normal factories; Space sources are all asteroid
chunk items. Space items can become the same item at any planet for zero cost.
Transfer is the only rocket edge; it requires a Rocket Silo recipe to produce
`RocketWeightCapacity` at the source planet.

## UI

The project header lists universe factories next to normal factories. `✦`
creates one; selecting it opens the universe workspace. The workspace can add
local mechanics, configure recipe/mining/fuel mechanics, add transfers, add
located targets, and solve with the project LP backend.
