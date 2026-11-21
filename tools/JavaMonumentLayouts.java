// Independent oracle for v0.16 Track F (original code, no game assets): runs
// Java 26.3's own OceanMonumentPieces.MonumentBuilding constructor from the
// pinned server jar (SHA-1 in tools/extract_java_item_fixtures.py) and prints
// each case's child pieces (class, mainDesign, bounding box, room index and
// final openings) and a WorldgenRandom decoration-seed draw, the facts
// tests/fixtures/java/26.3/monument_layouts.txt and src/world/monument/tests.rs
// pin. Needs JDK 25 and the server's bundled libraries on the classpath:
//   unzip -q server.jar 'META-INF/libraries/*' -d libs; unzip -p server.jar \
//     META-INF/versions/26.3/server-26.3.jar > server-26.3.jar
//   CP="server-26.3.jar:$(find libs -name '*.jar' | tr '\n' :)"
//   javac -cp "$CP" -d out tools/JavaMonumentLayouts.java
//   java -cp "$CP:out" MonumentLayouts | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //'
import java.lang.reflect.Field;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.Direction;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.structures.OceanMonumentPieces;

public class MonumentLayouts {
    public static void main(String[] a) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        long[][] cases = {{12345L, 5, -3}, {-987654321L, -40, 17}, {42L, 0, 0}, {7777777777L, 100, -200}};
        for (long[] c : cases) {
            long seed = c[0]; int cx = (int) c[1], cz = (int) c[2];
            WorldgenRandom r = new WorldgenRandom(new LegacyRandomSource(0L));
            r.setLargeFeatureSeed(seed, cx, cz);
            Direction o = Direction.Plane.HORIZONTAL.getRandomDirection(r);
            OceanMonumentPieces.MonumentBuilding b = new OceanMonumentPieces.MonumentBuilding(r, cx * 16 - 29, cz * 16 - 29, o);
            BoundingBox bb = b.getBoundingBox();
            System.out.println("case " + seed + " " + cx + " " + cz + " " + o.getName() + " " + box(bb));
            Field f = OceanMonumentPieces.MonumentBuilding.class.getDeclaredField("childPieces");
            f.setAccessible(true);
            for (Object p : (List<?>) f.get(b)) {
                StructurePiece sp = (StructurePiece) p;
                String design = "";
                for (Field d : sp.getClass().getDeclaredFields()) {
                    if (d.getName().equals("mainDesign")) { d.setAccessible(true); design = " " + d.getInt(sp); }
                }
                Field rd = null;
                Class<?> k = sp.getClass();
                while (k != null && rd == null) { try { rd = k.getDeclaredField("roomDefinition"); } catch (NoSuchFieldException e) { k = k.getSuperclass(); } }
                String room = "";
                if (rd != null) { rd.setAccessible(true); Object def = rd.get(sp); if (def != null) { Field idx = def.getClass().getDeclaredField("index"); idx.setAccessible(true); Field op = def.getClass().getDeclaredField("hasOpening"); op.setAccessible(true); boolean[] o2 = (boolean[]) op.get(def); StringBuilder s = new StringBuilder(); for (boolean x : o2) s.append(x ? '1' : '0'); room = " room " + idx.getInt(def) + " " + s; } }
                System.out.println("  " + sp.getClass().getSimpleName() + design + " " + box(sp.getBoundingBox()) + room);
            }
        }
        // Decoration randoms.
        WorldgenRandom w = new WorldgenRandom(new XoroshiroRandomSource(0L));
        long dec = w.setDecorationSeed(12345L, -64, 48);
        System.out.println("decoration " + dec);
        w.setFeatureSeed(dec, 24, 4);
        StringBuilder s = new StringBuilder("draws");
        for (int i = 0; i < 6; i++) s.append(" ").append(w.nextInt(3));
        for (int i = 0; i < 6; i++) s.append(" ").append(w.nextBoolean());
        s.append(" ").append(w.nextInt(4)).append(" ").append(w.nextLong());
        System.out.println(s);
    }

    static String box(BoundingBox b) {
        return b.minX() + " " + b.minY() + " " + b.minZ() + " " + b.maxX() + " " + b.maxY() + " " + b.maxZ();
    }
}
