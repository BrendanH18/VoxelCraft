// v1.0 final track F3: Java 26.3's random block model variant picks, as
// `ModelBlockRenderer.tesselateBlock` makes them: a `SingleThreadedRandomSource`
// seeded with `Mth.getSeed(pos)`, then `WeightedList.getRandomOrThrow`'s
// `nextInt(totalWeight)`. Prints one JSON line per position: x, y, z and the
// pick out of 4 and out of 16, and (for fire's multipart model, which reseeds
// every part with one `nextLong`) the floor's pick of 2 and a side's of 4.
//
// Run with the client jar on the class path:
//   java -cp client.jar:. JavaBlockVariantVectors
import net.minecraft.util.Mth;
import net.minecraft.world.level.levelgen.SingleThreadedRandomSource;

public class JavaBlockVariantVectors {
    public static void main(String[] args) {
        int[][] points = new int[400][];
        java.util.Random r = new java.util.Random(26_3L);
        for (int i = 0; i < points.length; i++) {
            if (i < 64) {
                points[i] = new int[] {i % 4 - 2, i / 16 - 64, i / 4 % 4 - 2};
            } else {
                points[i] = new int[] {r.nextInt(60_000_000) - 30_000_000, r.nextInt(384) - 64, r.nextInt(60_000_000) - 30_000_000};
            }
        }
        StringBuilder out = new StringBuilder("[\n");
        for (int i = 0; i < points.length; i++) {
            int[] p = points[i];
            long seed = Mth.getSeed(p[0], p[1], p[2]);
            SingleThreadedRandomSource a = new SingleThreadedRandomSource(seed);
            int four = a.nextInt(4);
            SingleThreadedRandomSource b = new SingleThreadedRandomSource(seed);
            int sixteen = b.nextInt(16);
            SingleThreadedRandomSource c = new SingleThreadedRandomSource(seed);
            long part = c.nextLong();
            c.setSeed(part);
            int floor = c.nextInt(2);
            c.setSeed(part);
            int side = c.nextInt(4);
            out.append(String.format(" [%d, %d, %d, %d, %d, %d, %d]%s\n", p[0], p[1], p[2], four, sixteen, floor, side, i + 1 < points.length ? "," : ""));
        }
        System.out.print(out.append("]\n"));
    }
}
