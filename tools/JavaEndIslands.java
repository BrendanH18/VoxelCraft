// The End's island shape, read by running the official Java 26.3 server's own
// End noise router (tools/extract_java_end_islands_fixtures.py compiles and
// runs this against the verified jar). Original code: it only calls the jar's
// public API and prints what it returns; nothing is copied.
//
// For each seed it prints:
//   outer SEED X Z BITS    EndIslandFunction (DensityFunctions.endOuterIslands)
//   islands SEED X Z BITS  the router's erosion, end/islands (main island max outer islands)
//   column SEED X Z RUNS   the noise column NoiseBasedChunkGenerator.getBaseColumn
//                          generates: the solid (end stone) y ranges, "a-b" inclusive,
//                          comma-separated, or "-" for none
// BITS is the float's raw bits in hex (Float.floatToRawIntBits).
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.NoiseColumn;
import net.minecraft.world.level.biome.TheEndBiomeSource;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.world.level.levelgen.densityfunction.DensityFunction;
import net.minecraft.world.level.levelgen.densityfunction.DensityFunctions;

public class JavaEndIslands {
    static final long[] SEEDS = {0L, 42L, -4172144997902289642L};

    static String bits(float f) {
        return Integer.toHexString(Float.floatToRawIntBits(f));
    }

    static String runs(NoiseColumn column) {
        StringBuilder out = new StringBuilder();
        int start = -1;
        for (int y = 0; y <= 256; y++) {
            boolean solid = y < 256 && column.getBlock(y).is(Blocks.END_STONE);
            if (solid && start < 0) {
                start = y;
            } else if (!solid && start >= 0) {
                if (out.length() > 0) {
                    out.append(',');
                }
                out.append(start).append('-').append(y - 1);
                start = -1;
            }
        }
        return out.length() == 0 ? "-" : out.toString();
    }

    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        HolderLookup.Provider lookup = VanillaRegistries.createWorldLookup();
        Holder<NoiseGeneratorSettings> settings =
            lookup.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.END);
        TheEndBiomeSource biomes = TheEndBiomeSource.create(lookup.lookupOrThrow(Registries.BIOME));
        NoiseBasedChunkGenerator generator = new NoiseBasedChunkGenerator(biomes, settings);
        // The End dimension type: min y 0, height 256.
        LevelHeightAccessor height = LevelHeightAccessor.create(0, 256);
        DensityFunction outer = DensityFunctions.endOuterIslands();
        for (long seed : SEEDS) {
            RandomState random = RandomState.create(lookup.lookupOrThrow(Registries.NOISE), seed, settings.value());
            DensityFunction islands = settings.value().noiseRouter().erosion();
            for (int i = -15; i <= 15; i++) {
                for (int j = -15; j <= 15; j++) {
                    int x = i * 131 - 7;
                    int z = j * 127 + 3;
                    System.out.println("outer " + seed + " " + x + " " + z + " "
                        + bits(random.sampleBlockValueUncached(outer, x, 0, z)));
                    System.out.println("islands " + seed + " " + x + " " + z + " "
                        + bits(random.sampleBlockValueUncached(islands, x, 0, z)));
                }
            }
            // The central island's middle and edge, then the first chunks
            // with island mass along eight rays out past the void.
            java.util.List<int[]> chunks = new java.util.ArrayList<>();
            chunks.add(new int[] {0, 0});
            chunks.add(new int[] {6, -2});
            for (int ray = 0; ray < 8 && chunks.size() < 5; ray++) {
                double a = ray * Math.PI / 4.0 + 0.3;
                for (int r = 66; r < 200; r++) {
                    int cx = (int) Math.floor(Math.cos(a) * r);
                    int cz = (int) Math.floor(Math.sin(a) * r);
                    if (random.sampleBlockValueUncached(islands, cx * 16 + 8, 0, cz * 16 + 8) > 0.0F) {
                        chunks.add(new int[] {cx, cz});
                        break;
                    }
                }
            }
            for (int[] c : chunks) {
                for (int dz = 0; dz < 16; dz++) {
                    for (int dx = 0; dx < 16; dx++) {
                        int x = c[0] * 16 + dx;
                        int z = c[1] * 16 + dz;
                        NoiseColumn column = generator.getBaseColumn(x, z, height, random);
                        System.out.println("column " + seed + " " + x + " " + z + " " + runs(column));
                    }
                }
            }
        }
    }
}
