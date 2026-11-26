// The Nether biome source's parameter points, read by running the official
// Java 26.3 server's own `nether` multi-noise preset
// (tools/extract_java_nether_fixtures.py compiles and runs this against the
// verified jar). Original code: it only calls the jar's public API and
// prints each point's quantized parameter ranges; nothing is copied.
import com.mojang.datafixers.util.Pair;
import java.lang.reflect.Method;
import java.util.function.Function;
import net.minecraft.SharedConstants;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList;

public class JavaNetherVectors {
    static String range(Climate.Parameter p) {
        return p.min() + " " + p.max();
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        // The preset's source provider is package-private: call it reflectively.
        Object provider = MultiNoiseBiomeSourceParameterList.Preset.NETHER.provider();
        Method apply = provider.getClass().getDeclaredMethod("apply", Function.class);
        apply.setAccessible(true);
        Function<ResourceKey<Biome>, ResourceKey<Biome>> identity = key -> key;
        Climate.ParameterList<ResourceKey<Biome>> list =
            (Climate.ParameterList<ResourceKey<Biome>>) apply.invoke(provider, identity);
        for (Pair<Climate.ParameterPoint, ResourceKey<Biome>> pair : list.values()) {
            Climate.ParameterPoint p = pair.getFirst();
            System.out.println("point " + pair.getSecond().identifier().getPath() + " " + range(p.temperature()) + " "
                + range(p.humidity()) + " " + range(p.continentalness()) + " " + range(p.erosion()) + " "
                + range(p.depth()) + " " + range(p.weirdness()) + " " + p.offset());
        }
    }
}
