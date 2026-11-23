// Independent JDK oracle for v0.17 Track C2: original code, behaviour data
// only. tools/extract_java_dispense_fixtures.py compiles it against the
// pinned Java 26.3 server jar (and the libraries its bundler carries) and
// runs it. It bootstraps the registries and prints one JSON object per line:
//
// - `item`: every item whose dispense behaviour is not the plain drop, as
//   `DispenserBlock.getDispenseMethod` picks it (the bootstrap registry,
//   then the equippable, sulfur-cube and spawn-egg fallbacks);
// - `projectile`: each projectile item's `ProjectileItem.DispenseConfig`
//   (power, uncertainty, level event, spawn offset for every facing);
// - `boat`: the width of each boat entity the dispenser places;
// - `container`: every block whose block entity (or `WorldlyContainerHolder`)
//   is a `Container`, with its slots per face and which probe items it
//   accepts or gives up through each face of an empty container;
// - `hopper`: `Hopper.SUCK_AABB`.
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import com.google.gson.JsonParser;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.tags.TagKey;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Position;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.dispenser.BlockSource;
import net.minecraft.core.dispenser.DispenseItemBehavior;
import net.minecraft.core.dispenser.ProjectileDispenseBehavior;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.Container;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.WorldlyContainer;
import net.minecraft.world.WorldlyContainerHolder;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ProjectileItem;
import net.minecraft.world.item.equipment.Equippable;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.ComposterBlock;
import net.minecraft.world.level.block.DispenserBlock;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.Hopper;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

public class JavaDispenseVectors {
    static String name(Item item) {
        return BuiltInRegistries.ITEM.getKey(item).getPath();
    }

    static String name(Block block) {
        return BuiltInRegistries.BLOCK.getKey(block).getPath();
    }

    static String direction(Direction d) {
        return d.getSerializedName();
    }

    static double round(double v) {
        return Math.round(v * 1e9) / 1e9;
    }

    static Map<String, JsonObject> readTags(ZipFile jar, String kind) throws Exception {
        Map<String, JsonObject> tags = new HashMap<>();
        String prefix = "data/minecraft/tags/" + kind + "/";
        for (ZipEntry entry : java.util.Collections.list(jar.entries())) {
            String n = entry.getName();
            if (n.startsWith(prefix) && n.endsWith(".json")) {
                try (InputStream in = jar.getInputStream(entry)) {
                    tags.put(n.substring(prefix.length(), n.length() - 5),
                        JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject());
                }
            }
        }
        return tags;
    }

    static Set<String> members(Map<String, JsonObject> tags, String name, Map<String, Set<String>> done) {
        Set<String> cached = done.get(name);
        if (cached != null) {
            return cached;
        }
        Set<String> out = new HashSet<>();
        for (var value : tags.get(name).getAsJsonArray("values")) {
            String id = value.isJsonObject() ? value.getAsJsonObject().get("id").getAsString() : value.getAsString();
            if (id.startsWith("#minecraft:")) {
                out.addAll(members(tags, id.substring("#minecraft:".length()), done));
            } else {
                out.add(id.substring("minecraft:".length()));
            }
        }
        done.put(name, out);
        return out;
    }

    static <T> void bindTags(String kind, Registry<T> registry) throws Exception {
        java.net.URI jarUri = Item.class.getProtectionDomain().getCodeSource().getLocation().toURI();
        Map<String, Set<TagKey<T>>> byElement = new HashMap<>();
        try (ZipFile jar = new ZipFile(new java.io.File(jarUri))) {
            Map<String, JsonObject> tags = readTags(jar, kind);
            Map<String, Set<String>> done = new HashMap<>();
            for (String tag : tags.keySet()) {
                TagKey<T> key = TagKey.create(registry.key(), Identifier.withDefaultNamespace(tag));
                for (String element : members(tags, tag, done)) {
                    byElement.computeIfAbsent(element, e -> new HashSet<>()).add(key);
                }
            }
        }
        Method bind = Holder.Reference.class.getDeclaredMethod("bindTags", java.util.Collection.class);
        bind.setAccessible(true);
        for (T element : registry) {
            Holder.Reference<T> holder = registry.get(registry.getResourceKey(element).get()).get();
            bind.invoke(holder, byElement.getOrDefault(registry.getKey(element).getPath(), Set.of()));
        }
    }

    // Probe stacks for the container faces: one of each kind of slot rule
    // the 26.3 containers test (fuel, buckets, brewing items, books, discs,
    // shulker boxes, compostables and plain blocks).
    static final String[] PROBES = {
        "coal", "bucket", "water_bucket", "lava_bucket", "iron_ore", "iron_ingot", "blaze_powder", "nether_wart",
        "potion", "glass_bottle", "book", "enchanted_book", "written_book", "music_disc_cat", "shulker_box",
        "red_shulker_box", "wheat_seeds", "stone", "bone_meal", "sponge", "wet_sponge",
    };

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        // Bootstrap alone leaves item components and tags unbound: bind the
        // default components as the data generator's component report does,
        // and the jar's item and block tags.
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS
            .build(VanillaRegistries.createWorldLookup())
            .forEach(DataComponentInitializers.PendingComponents::apply);
        bindTags("item", BuiltInRegistries.ITEM);
        bindTags("block", BuiltInRegistries.BLOCK);

        Field registryField = DispenserBlock.class.getDeclaredField("DISPENSER_REGISTRY");
        registryField.setAccessible(true);
        @SuppressWarnings("unchecked")
        Map<Item, DispenseItemBehavior> registry = (Map<Item, DispenseItemBehavior>) registryField.get(null);
        Method fallback = DispenserBlock.class.getDeclaredMethod("getDefaultDispenseMethod", ItemStack.class);
        fallback.setAccessible(true);
        Field defaultField = DispenserBlock.class.getDeclaredField("DEFAULT_BEHAVIOR");
        defaultField.setAccessible(true);
        Object plain = defaultField.get(null);

        // An anonymous bootstrap behaviour is named after the first item (in
        // registry order) it serves, so a shared instance keeps one name.
        Map<DispenseItemBehavior, String> anonymous = new IdentityHashMap<>();
        for (Item item : BuiltInRegistries.ITEM) {
            DispenseItemBehavior b = registry.get(item);
            if (b != null && b.getClass().isAnonymousClass() && !anonymous.containsKey(b)) {
                anonymous.put(b, "bootstrap:" + name(item));
            }
        }

        int plainCount = 0;
        for (Item item : BuiltInRegistries.ITEM) {
            ItemStack stack = new ItemStack(item);
            DispenseItemBehavior registered = registry.get(item);
            String behaviour;
            boolean fromRegistry = registered != null;
            if (registered != null) {
                behaviour = registered.getClass().isAnonymousClass()
                    ? anonymous.get(registered)
                    : registered.getClass().getSimpleName();
            } else {
                Object b = fallback.invoke(null, stack);
                if (b == plain) {
                    plainCount++;
                    continue;
                }
                behaviour = b.getClass().getSimpleName();
            }
            JsonObject line = new JsonObject();
            line.addProperty("type", "item");
            line.addProperty("item", name(item));
            line.addProperty("behaviour", behaviour);
            line.addProperty("registered", fromRegistry);
            line.addProperty("experimental", !item.isEnabled(FeatureFlags.DEFAULT_FLAGS));
            Equippable equippable = stack.get(DataComponents.EQUIPPABLE);
            if (equippable != null) {
                JsonObject e = new JsonObject();
                e.addProperty("slot", equippable.slot().getSerializedName());
                e.addProperty("dispensable", equippable.dispensable());
                if (equippable.allowedEntities().isPresent()) {
                    HolderSet<EntityType<?>> allowed = equippable.allowedEntities().get();
                    if (allowed.unwrapKey().isPresent()) {
                        e.addProperty("allowed", "#" + allowed.unwrapKey().get().location().getPath());
                    } else {
                        JsonArray list = new JsonArray();
                        allowed.forEach(h -> list.add(BuiltInRegistries.ENTITY_TYPE.getKey(h.value()).getPath()));
                        e.add("allowed", list);
                    }
                }
                line.add("equippable", e);
            }
            System.out.println(line);

            if (registered instanceof ProjectileDispenseBehavior) {
                ProjectileItem.DispenseConfig config = ((ProjectileItem) item).createDispenseConfig();
                JsonObject p = new JsonObject();
                p.addProperty("type", "projectile");
                p.addProperty("item", name(item));
                p.addProperty("power", config.power());
                p.addProperty("uncertainty", config.uncertainty());
                p.addProperty("event", config.overrideDispenseEvent().orElse(1002));
                JsonObject offsets = new JsonObject();
                for (Direction d : Direction.values()) {
                    BlockState state = net.minecraft.world.level.block.Blocks.DISPENSER.defaultBlockState()
                        .setValue(DispenserBlock.FACING, d);
                    BlockSource source = new BlockSource(null, BlockPos.ZERO, state, null);
                    Position at = config.positionFunction().getDispensePosition(source, d);
                    Vec3 centre = source.center();
                    JsonArray xyz = new JsonArray();
                    xyz.add(round(at.x() - centre.x));
                    xyz.add(round(at.y() - centre.y));
                    xyz.add(round(at.z() - centre.z));
                    offsets.add(direction(d), xyz);
                }
                p.add("offset", offsets);
                System.out.println(p);
            }
        }
        JsonObject summary = new JsonObject();
        summary.addProperty("type", "summary");
        summary.addProperty("plain_items", plainCount);
        summary.addProperty("registry_size", registry.size());
        System.out.println(summary);

        for (EntityType<?> boat : new EntityType<?>[] {EntityTypes.OAK_BOAT, EntityTypes.BAMBOO_RAFT}) {
            JsonObject b = new JsonObject();
            b.addProperty("type", "boat");
            b.addProperty("entity", BuiltInRegistries.ENTITY_TYPE.getKey(boat).getPath());
            b.addProperty("width", boat.getWidth());
            System.out.println(b);
        }

        AABB suck = Hopper.SUCK_AABB;
        JsonObject hopper = new JsonObject();
        hopper.addProperty("type", "hopper");
        JsonArray box = new JsonArray();
        for (double v : new double[] {suck.minX, suck.minY, suck.minZ, suck.maxX, suck.maxY, suck.maxZ}) {
            box.add(v);
        }
        hopper.add("suck_aabb", box);
        System.out.println(hopper);

        SimpleContainer taker = new SimpleContainer(5);
        for (Block block : BuiltInRegistries.BLOCK) {
            List<Container> containers = new ArrayList<>();
            List<String> labels = new ArrayList<>();
            if (block instanceof WorldlyContainerHolder holder) {
                for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                    containers.add(holder.getContainer(state, null, BlockPos.ZERO));
                    labels.add("level=" + state.getValue(ComposterBlock.LEVEL));
                }
            } else if (block instanceof EntityBlock entityBlock) {
                BlockEntity entity = entityBlock.newBlockEntity(BlockPos.ZERO, block.defaultBlockState());
                if (entity instanceof Container c) {
                    containers.add(c);
                    labels.add("");
                }
            }
            for (int i = 0; i < containers.size(); i++) {
                Container c = containers.get(i);
                JsonObject line = new JsonObject();
                line.addProperty("type", "container");
                line.addProperty("block", name(block));
                line.addProperty("state", labels.get(i));
                line.addProperty("class", c.getClass().getSimpleName().isEmpty()
                    ? c.getClass().getName() : c.getClass().getSimpleName());
                line.addProperty("size", c.getContainerSize());
                line.addProperty("worldly", c instanceof WorldlyContainer);
                line.addProperty("max_stack", c.getMaxStackSize());
                JsonObject faces = new JsonObject();
                for (Direction d : Direction.values()) {
                    int[] slots;
                    if (c instanceof WorldlyContainer w) {
                        slots = w.getSlotsForFace(d);
                    } else {
                        slots = new int[c.getContainerSize()];
                        for (int s = 0; s < slots.length; s++) {
                            slots[s] = s;
                        }
                    }
                    JsonArray list = new JsonArray();
                    for (int slot : slots) {
                        JsonObject entry = new JsonObject();
                        entry.addProperty("slot", slot);
                        JsonArray place = new JsonArray();
                        JsonArray take = new JsonArray();
                        for (String probe : PROBES) {
                            Item item = BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(probe));
                            ItemStack stack = new ItemStack(item);
                            boolean canPlace;
                            try {
                                canPlace = c.canPlaceItem(slot, stack)
                                    && !(c instanceof WorldlyContainer w && !w.canPlaceItemThroughFace(slot, stack, d));
                            } catch (NullPointerException missingLevel) {
                                canPlace = false;
                                line.addProperty("needs_level", true);
                            }
                            if (canPlace) {
                                place.add(probe);
                            }
                            boolean canTake = c.canTakeItem(taker, slot, stack)
                                && !(c instanceof WorldlyContainer w && !w.canTakeItemThroughFace(slot, stack, d));
                            if (canTake) {
                                take.add(probe);
                            }
                        }
                        entry.add("place", place);
                        entry.add("take", take);
                        list.add(entry);
                    }
                    faces.add(direction(d), list);
                }
                line.add("faces", faces);
                System.out.println(line);
            }
        }
    }
}
