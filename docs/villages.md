# Villages and villagers

Village content uses append-only block states 900–952. All workstation
blocks have original procedural textures. Barrels use the saved 27-slot
chest inventory, shared by host, controller players and CLI agents.
Smokers cook food and blast furnaces smelt ores/raw metals/ancient debris
in five seconds, burning fuel twice as fast (eight items per coal).
Their inventories, progress and XP survive saves, and lit states emit 13.

Grindstones use two inputs and a result in the host/controller screens.
CLI agents use `grindstone [second-hotbar-slot]` while targeting one.
Repair combines remaining durability plus floor(5% maximum durability).
Non-curse enchantments are removed, curses and custom names survive,
uncursed enchanted books become books, and prior work resets according
to the retained curses. XP is uniformly chosen from ceil(sum/2) through
2*ceil(sum/2)-1, where sum is the removed enchantments' minimum cost.
Taking the result consumes inputs once and releases XP orbs.

Craftable: job sites, hay bales and fast furnaces. Bells are village loot /
creative only, as in Java. A shovel makes dirt paths; paths are 15/16 high
and drop dirt. Hay bales support three placement axes.

Current simplifications: composter processing, lectern books, loom banners,
cartography maps, stonecutter UI, wall/ceiling grindstones, vertical barrel
facings, bell ringing and hay-bale fall cushioning are not implemented.
Blast-furnace equipment recycling is not yet implemented.

## Java references

- [GrindstoneMenu](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/inventory/GrindstoneMenu.java)
- [AbstractFurnaceBlockEntity](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/entity/AbstractFurnaceBlockEntity.java)
- [Java 1.21 data](https://github.com/InventivetalentDev/minecraft-assets/tree/1.21/data/minecraft)
- [RandomSpreadStructurePlacement](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/structure/placement/RandomSpreadStructurePlacement.java)
- [JigsawStructure](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/structure/structures/JigsawStructure.java)
- [Villager](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/npc/Villager.java)

## Generation

`/locate village` or `/locate structure village` locates the nearest valid
start. Only the Overworld generates villages: plains, desert, savanna,
taiga and the generator's snowy biome map directly to the five Java sets.
Placement uses Java's 16-block chunks, 34 spacing, 8 separation,
10387312 salt, linear spread and legacy Java random, including negative
regions. Noise remains VoxelCraft's own, so starts are not vanilla seeds.

A well/meeting point grows streets through oriented connectors, then houses
and farms; branching stops at depth 6 and 80 blocks from the centre. Piece
boxes cannot overlap. Streets follow deterministic terrain columns; rigid
houses sit at their entrances and fill foundations downward. Cliffs/water
are rejected. Each chunk paints only its portion of the cached layout,
including foundations and clearing tree canopies above pieces. Eviction
and generation order do not change layouts.

Palettes use oak/cobble, sandstone, acacia/terracotta, spruce/cobble, and
spruce/snow roofs. Houses contain beds, a job site, a lit doorway and a
chest; farms have wheat/carrot/potato rows, irrigation, hay and composters.
Generated containers register immediately; saved entries override seeded
loot, including empty looted chests. House loot follows Java 1.21 biome
weights, 3–8 rolls and item counts. Unsupported loot remains weighted
empty rolls (berries, some seeds, saddles, signs, blue ice, beetroot soup).

The compact house/street/farm/meeting pools are original geometry inspired
by Java's jigsaw pools. They do not reproduce the full vanilla NBT template
catalogue, pool weights, processor lists, abandoned variants, decorative
pools, terrain beard blending or specialized profession chest tables.
Snow roofs currently use full snow blocks because snow layers are absent.

Baseline on Apple M5, `--bench --rd 4`: 0.228 ms/generated chunk,
0.678 ms/dense chunk for light+mesh. Overlap painting, signed placement,
all five biome sets, seeded loot, cache regeneration and saved-container
precedence have regression coverage.

After village generation, the same idle-system benchmark measured 0.210
ms/generated chunk and 0.663 ms/dense light+mesh chunk (no material
regression; this fixed benchmark volume does not intersect a village).

## Residents and trading

Each generated house registers one resident and a permanent birth receipt.
Receipts are saved even if the villager dies, so regenerating an unmodified
chunk cannot duplicate it. One in five procedural residents starts as a baby;
babies grow over 24000 loaded simulation ticks (20 minutes), claim beds, and
wait until adulthood to claim jobs. Villagers have Java's 20 health, 0 kill XP,
crossed arms, noses, profession aprons, farmer hats and original nasal voices.

Adults claim the nearest unclaimed loaded job site within 48 blocks. Twelve
job-site professions are supported: farmer, fisherman, shepherd, fletcher,
librarian, cartographer, cleric, armorer, weaponsmith, toolsmith, butcher and
mason. Breaking a job site releases it. Untraded villagers become unemployed;
a villager with trade XP retains its profession and seeks a matching site.
Beds of every supported colour are claimed exclusively among villagers.
Sensors run once a second without allocating. Work runs from day tick 2000
to 9000; rest starts at 12000. Residents walk toward work/home, open wooden
doors, sleep near their beds, and flee zombies/husks/drowned within eight
blocks. Sleeping villagers lie down. Unloaded or distant residents freeze
and remain saved; their POIs leave the loaded index when chunks unload.

Right-click a villager to see prices, stock, rank and level progress. Controller
LT opens its own Trading tab; A/X/Y on an offer completes one trade into that
player's inventory. CLI `trade` inspects the targeted merchant in the response's
`state.merchant`, and `trade 1` through `trade 10` buy its numbered offers.
The host console also accepts `/trade [1..10]`. Payments (including librarian
books) and result capacity are checked atomically. Insufficient payment, full
inventory, dead/baby/sleeping villagers, and depleted stock consume nothing.
Opening a screen holds that merchant still; leaving reach closes it.
`--open-trading` opens the targeted merchant after job acquisition for captures,
including a virtual `--pad-player play` screen.

Normal Java trade definitions keep their quantities, stock caps, villager XP
and price multipliers. Two distinct supported offers unlock per tier where
available. Novice/Apprentice/Journeyman/Expert/Master thresholds are 0/10/70/150/250
XP. Enchanted gear and librarian books use Java enchantment levels and price
ranges; treasure books cost double. Successful trades release 3–6 player XP,
plus 5 on a promotion. At most two restocks occur per day, while near the job
site during work hours, separated by 2400 day ticks. Restocking updates Java's
supply/demand term and resets uses. Offers, enchantments, demand, stock, rank,
XP, bed/job claims, age, identity and restock counters survive dimension saves.
Older saves without the additive `villagers` property remain valid.

Simplifications: procedural house templates/resident ages rather than vanilla
NBT assets; straight-line navigation with obstacle sidesteps rather than full
brain/pathfinding; sleep near a bed rather than snapping to its exact facing;
doors remain open. Trade selection uses an independent deterministic seed
stream, promotions unlock immediately rather than after 40 ticks, and the
restock interval is a fixed day-time interval. Item families absent from the
engine are filtered from trade pools, so some tiers offer fewer than two trades
(or none): fish/buckets, campfires, maps, banners, item frames, suspicious stew,
glazed terracotta, tipped arrows, and unavailable foods/stone variants. The
leatherworker has no cauldron job site yet. Villager breeding/food inventories,
gossip, reputation/curing discounts, raids, zombie attacks on villagers, iron
golems, zombie villagers/curing and wandering traders remain gaps. Profession
aprons share an original base model across biomes; level badges are in the UI.

Additional Java references:
- [Normal trade tables and constructors](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/npc/VillagerTrades.java)
- [Villager schedules](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/schedule/Schedule.java)
- [POI acquisition](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/ai/behavior/AcquirePoi.java)
- [Merchant stock and demand](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/item/trading/MerchantOffer.java)

## Round 2 review: golems and zombie villagers

Player builds require a carved pumpkin or jack o'lantern placed last, two
snow blocks or the four-block iron T, including the iron pattern's empty
corners. Golems persist across unloading/saves. Iron golems have 100 health,
random 7.5–21.5 monster damage, player difficulty scaling, upward knockback,
crack stages and 25-health ingot repairs. Player-built golems never attack
players; other golems retaliate. They target nearby hostiles except creepers.
Creepers do not flee golems in Java. Snow golems throw snowballs, suffer water
and desert damage. Snow-layer trails await a snow-layer block implementation.

Village summoning requires recent sleep, three eligible villagers panicking
or five on a gossip check, no nearby golem, and a supported unobstructed spawn
position. Remaining simplifications: a cluster-wide 30-second cooldown,
spherical sensors and minute gossip checks rather than the complete Java
brain memory/sensor scheduling. Snow golems' heat damage currently uses the
desert biome rather than the complete biome temperature system.

Zombies/husks/drowned attack villagers. Fatal attacks infect on Normal (50%)
and Hard (100%); Easy/Peaceful never infect. Natural zombies everywhere have
a 5% zombie-villager roll. Weakness plus a regular golden apple starts a
3600–6000-tick cure, removes Weakness and keeps the zombie persistent. Trades,
profession, XP, age, armor, weakness/conversion timers and golem health/build
state survive saves. Baby zombies remain babies until curing, then grow as
villagers. Cures give a capped permanent trade discount; per-player gossip,
conversion Strength, bed/bar acceleration and equipment-drop handling are
still to be completed. Summoning and combat use reused spatial buckets rather
than per-mob full entity scans; collision broad phase shares that index.

## Breeding

Residents pick up dropped bread, carrots, potatoes and beetroot into eight
persistent slots. Bread is worth four food points, the vegetables one; both
parents need twelve. Awake, safe adults approach each other and court for
275–324 ticks with hearts. A free bed within 48 blocks and two empty blocks
above its head is required. Parents consume twelve points, receive a 6000-tick
cooldown on success, and the child claims the bed at age -24000. Babies grow
in loaded simulation time; food and parent cooldowns survive saves. Failed
bed checks produce angry particles. Beetroot is available as a food item;
beetroot crops and farmer harvesting are not implemented yet. Food pickup
obeys mobGriefing. Residents share excess stacks using Java's above-32 half
stack / above-24 excess rule, directly into a nearby hungry resident's
inventory instead of throwing an item entity.
Navigation uses the existing resident obstacle sidesteps; a complete Java
path-reachability test for beds is still absent.

## Composters

Composters have saved block levels 0–8 (filled states 961–968). Eligible
items use Java's 30/50/65/85/100 percent chances; the first level always
succeeds. One input is consumed even on a failed roll. Level 7 waits twenty
ticks before becoming ready (8), then use releases one bone meal and resets
it. Readiness ticks survive saves and unloaded chunks wait to process them.
The hollow collision/render shape fills with compost, with original compost
and ready textures in the village band. Comparators read the level directly.
Top hoppers insert compostables; bottom hoppers extract ready bone meal;
side insertion/extraction fails. All filled states remain farmer job sites.
Desktop, controller and CLI players share the composter action. Recipes and
workstation acquisition retain their existing Java behavior.

## Bells

Use rings a bell, as does a rising redstone edge; held power does not repeat.
An original metallic chime plays, and villagers within 32 blocks interrupt
work/trading and walk to their claimed beds to hide for fifteen seconds.
Hiding time survives saves. Bells are meeting POIs for trader spawning.
Raid detection/glow is deferred with raids, and the fixed bell geometry does
not yet swing. The existing floor-mounted model/orientation is retained.

## Wandering traders

Overworld traders attempt every 24000 ticks with a 25/50/75 percent roll
and a further one-in-ten roll (2.5/5/7.5 percent effective chance), resetting
on success. Attempts choose a player and prefer a bell within 48 blocks;
ten candidate ground locations within 48 blocks need clear spawn space.
Two trader llamas attempt nearby spawns, follow their merchant through a
reused ID index, and share its despawn lifetime. Trader event spawning is
independent of difficulty, gated by doMobSpawning and doTraderSpawning.

Traders offer five distinct common and one rare supported Java 1.21 offer,
with exact quantities/prices/stock caps, no leveling or restocking, through
the shared merchant UI/controller/CLI transaction. They disappear after
48000 loaded ticks (paused while trading), drink for 32 ticks to become
invisible at night, and drink milk to reappear by day. Offers, remaining
lifetime, destination, potion state, llama links and attempt progression
survive saves. Models and robes are original procedural geometry.

Remaining simplifications: trade pools filter absent items, shared resident
navigation, llama following without physical/rendered leads, no llama
spitting/taming/riding, and no held potion/milk model or drinking sound.
Biome exclusions await the unsupported void/deep-dark biome tags. A trader
can remain frozen in a loaded chunk beyond the common 128-block activation
radius, as other persistent village entities currently do.

## Reputation

Gossip is stored per player with Java's five weighted types and caps. Curing
records the initiating player, grants major-positive 20 and minor-positive 25
(125 initial reputation, 100 after the temporary bonus decays), and repeat
cures do not stack. Successful trades add trading gossip; melee harm and
nearby witnessed kills add negative gossip. Daily decay and nearby minute
sharing preserve type-specific rules, including non-transferable permanent
cure gossip. Host, controller and CLI prices/payments use the same player's
reputation, and gossip/cure identity survive saves. Older single-number
reputation migrates as a host-only legacy discount. Sharing currently visits
the closest resident and transfers all eligible entries rather than Java's
weighted selection of ten. Golem reputation-based hostility and gossip from
projectile/magic damage are still absent.
