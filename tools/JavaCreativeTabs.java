// Independent JDK oracle for v0.18 Track 4: original code, behaviour data
// only. tools/extract_java_creative_fixtures.py compiles it against the
// pinned Java 26.3 server jar (and the libraries its bundler carries) and
// runs it. It bootstraps the registries, builds every creative tab with
// `CreativeModeTabs.tryRebuildTabContents` (the default feature flags, with
// and without operator permissions) and prints one JSON object per line:
//
// - `tab`: each tab in registry order with its row, column, type, alignment,
//   title key, icon, scroll and title flags, and its display and search-tab
//   stacks (item id plus the stack's component patch as JSON);
// - `egg`: every spawn egg item: the entity type it spawns, that type's
//   category and whether it may spawn in Peaceful.
//
// The client receives data-driven registries (enchantments, painting
// variants, instruments, banner patterns) sorted by id, because the server
// loads them from the data pack's file listing; the oracle's lookup lists
// those registries in id order too, and binds the jar's tags in file order.
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.InputStreamReader;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Stream;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.SpawnEggItem;
import net.minecraft.world.item.component.TypedEntityData;

public class JavaCreativeTabs {
    static final Map<String, Map<String, List<String>>> TAGS = new HashMap<>();

    static ZipFile jar() throws Exception {
        java.net.URI uri = Item.class.getProtectionDomain().getCodeSource().getLocation().toURI();
        return new ZipFile(new java.io.File(uri));
    }

    /** Tag name to member ids, in file order with nested tags expanded in place. */
    static Map<String, List<String>> readTags(String kind) throws Exception {
        Map<String, JsonObject> raw = new HashMap<>();
        String prefix = "data/minecraft/tags/" + kind + "/";
        try (ZipFile jar = jar()) {
            var entries = jar.entries();
            while (entries.hasMoreElements()) {
                ZipEntry e = entries.nextElement();
                if (e.getName().startsWith(prefix) && e.getName().endsWith(".json")) {
                    String name = e.getName().substring(prefix.length(), e.getName().length() - 5);
                    try (var r = new InputStreamReader(jar.getInputStream(e), StandardCharsets.UTF_8)) {
                        raw.put(name, JsonParser.parseReader(r).getAsJsonObject());
                    }
                }
            }
        }
        Map<String, List<String>> out = new HashMap<>();
        for (String name : raw.keySet()) {
            out.put(name, expand(raw, name));
        }
        return out;
    }

    static List<String> expand(Map<String, JsonObject> raw, String name) {
        List<String> out = new ArrayList<>();
        for (JsonElement value : raw.get(name).getAsJsonArray("values")) {
            String id = value.isJsonObject() ? value.getAsJsonObject().get("id").getAsString() : value.getAsString();
            if (id.startsWith("#minecraft:")) {
                for (String member : expand(raw, id.substring("#minecraft:".length()))) {
                    if (!out.contains(member)) {
                        out.add(member);
                    }
                }
            } else if (!out.contains(id)) {
                out.add(id);
            }
        }
        return out;
    }

    static <T> void bindHolderTags(String kind, Registry<T> registry) throws Exception {
        Map<String, List<String>> tags = readTags(kind);
        Map<String, Set<TagKey<T>>> byElement = new HashMap<>();
        for (var entry : tags.entrySet()) {
            TagKey<T> key = TagKey.create(registry.key(), Identifier.withDefaultNamespace(entry.getKey()));
            for (String id : entry.getValue()) {
                byElement.computeIfAbsent(id, e -> new HashSet<>()).add(key);
            }
        }
        Method bind = Holder.Reference.class.getDeclaredMethod("bindTags", java.util.Collection.class);
        bind.setAccessible(true);
        for (T element : registry) {
            Holder.Reference<T> holder = registry.get(registry.getResourceKey(element).get()).get();
            bind.invoke(holder, byElement.getOrDefault(registry.getKey(element).toString(), Set.of()));
        }
    }

    /**
     * A data-driven registry as the client sees it: elements in id order, the
     * jar's tags bound to each holder and listed in file order.
     */
    static <T> HolderLookup.RegistryLookup<T> sorted(HolderLookup.RegistryLookup<T> parent, String tagKind) throws Exception {
        Map<String, List<String>> tags = tagKind == null ? Map.of() : readTags(tagKind);
        Method bindHolder = Holder.Reference.class.getDeclaredMethod("bindTags", java.util.Collection.class);
        bindHolder.setAccessible(true);
        Map<String, Set<TagKey<T>>> byElement = new HashMap<>();
        for (var entry : tags.entrySet()) {
            @SuppressWarnings("unchecked")
            ResourceKey<? extends Registry<T>> registry = (ResourceKey<? extends Registry<T>>) (ResourceKey<?>) parent.key();
            TagKey<T> key = TagKey.create(registry, Identifier.withDefaultNamespace(entry.getKey()));
            for (String id : entry.getValue()) {
                byElement.computeIfAbsent(id, e -> new HashSet<>()).add(key);
            }
        }
        List<Holder.Reference<T>> elements = parent.listElements()
            .sorted(Comparator.comparing(h -> h.key().identifier().toString()))
            .toList();
        for (Holder.Reference<T> h : elements) {
            bindHolder.invoke(h, byElement.getOrDefault(h.key().identifier().toString(), Set.of()));
        }
        Method bindSet = HolderSet.Named.class.getDeclaredMethod("bind", List.class);
        bindSet.setAccessible(true);
        // `HolderSet.emptyNamed` never dereferences; build a plain named set.
        var named = HolderSet.Named.class.getDeclaredConstructor(net.minecraft.core.HolderOwner.class, TagKey.class);
        named.setAccessible(true);
        return new HolderLookup.RegistryLookup.Delegate<T>() {
            @Override
            public HolderLookup.RegistryLookup<T> parent() {
                return parent;
            }

            @Override
            public Stream<Holder.Reference<T>> listElements() {
                return elements.stream();
            }

            @Override
            public Optional<HolderSet.Named<T>> get(TagKey<T> key) {
                List<String> ids = tags.get(key.location().getPath());
                if (ids == null) {
                    return Optional.empty();
                }
                List<Holder<T>> members = new ArrayList<>();
                for (String id : ids) {
                    elements.stream().filter(h -> h.key().identifier().toString().equals(id)).findFirst().ifPresent(members::add);
                }
                try {
                    @SuppressWarnings("unchecked")
                    HolderSet.Named<T> set = (HolderSet.Named<T>) named.newInstance(parent, key);
                    bindSet.invoke(set, members);
                    return Optional.of(set);
                } catch (Exception e) {
                    throw new RuntimeException(e);
                }
            }
        };
    }

    static final Map<String, String> DATA_DRIVEN = Map.of(
        "enchantment", "enchantment",
        "painting_variant", "painting_variant",
        "instrument", "instrument",
        "banner_pattern", "banner_pattern"
    );

    static String id(Item item) {
        return BuiltInRegistries.ITEM.getKey(item).getPath();
    }

    static JsonArray stacks(Iterable<ItemStack> stacks, RegistryOps<JsonElement> ops) {
        JsonArray out = new JsonArray();
        for (ItemStack stack : stacks) {
            JsonObject entry = new JsonObject();
            entry.addProperty("item", id(stack.getItem()));
            DataComponentPatch patch = stack.getComponentsPatch();
            if (!patch.isEmpty()) {
                entry.add("components", DataComponentPatch.CODEC.encodeStart(ops, patch).getOrThrow());
            }
            out.add(entry);
        }
        return out;
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        HolderLookup.Provider world = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(world).forEach(DataComponentInitializers.PendingComponents::apply);
        bindHolderTags("item", BuiltInRegistries.ITEM);
        bindHolderTags("block", BuiltInRegistries.BLOCK);
        bindHolderTags("entity_type", BuiltInRegistries.ENTITY_TYPE);

        List<HolderLookup.RegistryLookup<?>> lookups = new ArrayList<>();
        for (var key : world.listRegistryKeys().toList()) {
            HolderLookup.RegistryLookup<?> lookup = world.lookupOrThrow(key);
            String path = key.identifier().getPath();
            lookups.add(DATA_DRIVEN.containsKey(path) ? sorted(lookup, DATA_DRIVEN.get(path)) : lookup);
        }
        HolderLookup.Provider client = HolderLookup.Provider.create(lookups.stream());
        RegistryOps<JsonElement> ops = client.createSerializationContext(JsonOps.INSTANCE);

        for (boolean op : new boolean[] {false, true}) {
            // Clear the cached parameters so the second build sees the permission.
            var cached = CreativeModeTabs.class.getDeclaredField("CACHED_PARAMETERS");
            cached.setAccessible(true);
            cached.set(null, null);
            CreativeModeTabs.tryRebuildTabContents(FeatureFlags.DEFAULT_FLAGS, op, client);
            for (CreativeModeTab tab : CreativeModeTabs.allTabs()) {
                JsonObject out = new JsonObject();
                out.addProperty("type", "tab");
                out.addProperty("permissions", op);
                out.addProperty("id", BuiltInRegistries.CREATIVE_MODE_TAB.getKey(tab).getPath());
                out.addProperty("row", tab.row().name().toLowerCase());
                out.addProperty("column", tab.column());
                out.addProperty("tab_type", tab.getType().name().toLowerCase());
                out.addProperty("aligned_right", tab.isAlignedRight());
                out.addProperty("show_title", tab.showTitle());
                out.addProperty("can_scroll", tab.canScroll());
                out.addProperty("should_display", tab.shouldDisplay());
                var contents = tab.getDisplayName().getContents();
                out.addProperty("title_key", contents instanceof net.minecraft.network.chat.contents.TranslatableContents t ? t.getKey() : contents.toString());
                out.addProperty("icon", id(tab.getIconItem().getItem()));
                out.addProperty("background", tab.getBackgroundTexture().toString());
                out.add("display", stacks(tab.getDisplayItems(), ops));
                out.add("search", stacks(tab.getSearchTabDisplayItems(), ops));
                System.out.println(out);
            }
        }

        for (Item item : BuiltInRegistries.ITEM) {
            if (!(item instanceof SpawnEggItem)) {
                continue;
            }
            EntityType<?> type = SpawnEggItem.getType(new ItemStack(item));
            JsonObject out = new JsonObject();
            out.addProperty("type", "egg");
            out.addProperty("item", id(item));
            out.addProperty("entity", BuiltInRegistries.ENTITY_TYPE.getKey(type).getPath());
            out.addProperty("category", type.getCategory().getName());
            out.addProperty("allowed_in_peaceful", type.isAllowedInPeaceful());
            out.addProperty("max_stack", new ItemStack(item).getMaxStackSize());
            System.out.println(out);
        }
        System.out.println("{\"type\":\"done\"}");
    }
}
