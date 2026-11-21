// Independent JDK oracle for v0.16 Track H: original code, behaviour data
// only. Run against the pinned Java 26.3 server jar (and the libraries its
// bundler carries) by tools/extract_java_wave1_leftovers_fixtures.py: it
// prints the draws `PotentSulfurBlockEntity.geyserPositional` makes, as
// JSON lines `[seed, x, y, z, dormant_draw, erupting_draw]`.
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;

public class JavaGeyserVectors {
    static final long SALT = -904011478L;

    static RandomSource positional(long seed, int x, int y, int z) {
        return new XoroshiroRandomSource(seed ^ SALT).forkPositional().at(x, y, z);
    }

    public static void main(String[] args) {
        long[] seeds = {0L, 7L, 12345L, -7L, 4_242_424_242L};
        int[][] cells = {{0, 0, 0}, {4, 10, 4}, {10, -20, 30}, {-123, 45, 678}, {30_000, -60, -30_000}};
        for (long seed : seeds) {
            for (int[] p : cells) {
                // Dormant: `nextIntBetweenInclusive(15, 30)`.
                int dormant = positional(seed, p[0], p[1], p[2]).nextIntBetweenInclusive(15, 30);
                // Erupting: one `nextInt()` discarded, then 1..=2.
                RandomSource r = positional(seed, p[0], p[1], p[2]);
                r.nextInt();
                int erupting = r.nextIntBetweenInclusive(1, 2);
                System.out.println("[" + seed + "," + p[0] + "," + p[1] + "," + p[2] + "," + dormant + "," + erupting + "]");
            }
        }
    }
}
