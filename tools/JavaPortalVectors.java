// Nether portal travel facts, read by running the official Java 26.3
// server's own code (tools/extract_java_portal_fixtures.py compiles and runs
// this against the verified jar). Original code: it only calls the jar's
// public API (and reads two private constants reflectively) and prints the
// results; nothing is copied.
//
// Output lines:
//   constant <name> <value>
//   border <minX> <maxX> <minZ> <maxZ>
//   within <x> <z> <true|false>
//   clamp <x> <y> <z> <bx> <by> <bz>
//   relative <minX> <minY> <minZ> <size1> <size2> <axis> <px> <py> <pz> <width> <height> <rx> <ry> <rz>
//   rectangle <case> <minX> <minY> <minZ> <size1> <size2>
import java.lang.reflect.Field;
import java.util.HashSet;
import java.util.Set;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.BlockUtil;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.portal.PortalForcer;
import net.minecraft.world.level.portal.PortalShape;
import net.minecraft.world.phys.Vec3;

public class JavaPortalVectors {
    static int constant(Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f.getInt(null);
    }

    static void relative(int x, int y, int z, int s1, int s2, Direction.Axis axis, double px, double py, double pz,
            float w, float h) {
        BlockUtil.FoundRectangle r = new BlockUtil.FoundRectangle(new BlockPos(x, y, z), s1, s2);
        Vec3 v = PortalShape.getRelativePosition(r, axis, new Vec3(px, py, pz), EntityDimensions.scalable(w, h));
        System.out.println("relative " + x + " " + y + " " + z + " " + s1 + " " + s2 + " " + axis.getName() + " "
            + px + " " + py + " " + pz + " " + w + " " + h + " " + v.x + " " + v.y + " " + v.z);
    }

    /** A portal plane along `axis`: cells (along, up) relative to (0, 64, 0). */
    static void rectangle(String name, Direction.Axis axis, int[][] cells, int[] center) {
        Set<BlockPos> set = new HashSet<>();
        for (int[] c : cells) {
            set.add(axis == Direction.Axis.X ? new BlockPos(c[0], 64 + c[1], 0) : new BlockPos(0, 64 + c[1], c[0]));
        }
        BlockPos start = axis == Direction.Axis.X ? new BlockPos(center[0], 64 + center[1], 0)
            : new BlockPos(0, 64 + center[1], center[0]);
        BlockUtil.FoundRectangle r = BlockUtil.getLargestRectangleAround(start, axis, 21, Direction.Axis.Y, 21, set::contains);
        System.out.println("rectangle " + name + " " + r.minCorner.getX() + " " + r.minCorner.getY() + " "
            + r.minCorner.getZ() + " " + r.axis1Size + " " + r.axis2Size);
    }

    static int[][] box(int w, int h, int x0, int y0) {
        int[][] out = new int[w * h][];
        for (int i = 0; i < w; i++) {
            for (int j = 0; j < h; j++) {
                out[i * h + j] = new int[] {x0 + i, y0 + j};
            }
        }
        return out;
    }

    static int[][] join(int[][] a, int[][] b) {
        int[][] out = new int[a.length + b.length][];
        System.arraycopy(a, 0, out, 0, a.length);
        System.arraycopy(b, 0, out, a.length, b.length);
        return out;
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.out.println("constant nether_portal_radius " + constant(PortalForcer.class, "NETHER_PORTAL_RADIUS"));
        System.out.println("constant overworld_portal_radius " + constant(PortalForcer.class, "OVERWORLD_PORTAL_RADIUS"));

        WorldBorder border = new WorldBorder();
        System.out.println("border " + border.getMinX() + " " + border.getMaxX() + " " + border.getMinZ() + " " + border.getMaxZ());
        int[][] within = {{0, 0}, {29999983, 0}, {29999984, 0}, {-29999984, 7}, {-29999985, 7}, {5, -29999984},
            {5, -29999985}, {5, 29999983}, {5, 29999984}};
        for (int[] p : within) {
            System.out.println("within " + p[0] + " " + p[1] + " " + border.isWithinBounds(new BlockPos(p[0], 64, p[1])));
        }
        double[][] clamps = {{12.7, 70.2, -3.4}, {-0.5, 64.0, 0.5}, {3.0e7, 80.9, -3.0e7}, {29999983.5, 64.0, -29999984.5},
            {-2.4e8, 30.0, 2.4e8}, {29999984.0, 10.0, 29999983.99}};
        for (double[] p : clamps) {
            BlockPos b = border.clampToBounds(p[0], p[1], p[2]);
            System.out.println("clamp " + p[0] + " " + p[1] + " " + p[2] + " " + b.getX() + " " + b.getY() + " " + b.getZ());
        }

        // Entry offsets: a 2 x 3 portal along X and Z, a wide and tall one,
        // positions inside, at the edges and outside, standing and sneaking
        // players, and an entity wider than the portal.
        Direction.Axis X = Direction.Axis.X, Z = Direction.Axis.Z;
        relative(10, 64, -3, 2, 3, X, 11.0, 64.0, -2.5, 0.6f, 1.8f);
        relative(10, 64, -3, 2, 3, X, 10.3, 64.0, -2.2, 0.6f, 1.8f);
        relative(10, 64, -3, 2, 3, X, 11.7, 65.2, -2.9, 0.6f, 1.8f);
        relative(10, 64, -3, 2, 3, X, 9.2, 63.0, -1.7, 0.6f, 1.8f);
        relative(10, 64, -3, 2, 3, X, 12.9, 70.0, -3.4, 0.6f, 1.5f);
        relative(-7, 40, 21, 2, 3, Z, -6.5, 40.0, 22.25, 0.6f, 1.8f);
        relative(-7, 40, 21, 2, 3, Z, -6.8, 40.6, 21.3, 0.6f, 1.8f);
        relative(-7, 40, 21, 5, 7, Z, -6.1, 43.5, 24.9, 0.6f, 1.8f);
        relative(-7, 40, 21, 21, 21, Z, -7.4, 50.0, 30.0, 0.6f, 1.5f);
        relative(100, 70, 100, 2, 3, X, 101.0, 70.5, 100.5, 3.9f, 4.0f);
        relative(100, 70, 100, 4, 3, X, 103.0, 71.0, 100.5, 1.4f, 2.9f);

        // Largest rectangles: plain portals, a portal joined to a second one,
        // a ragged blob, and one past the 21 limit.
        rectangle("plain_x", X, box(2, 3, 0, 0), new int[] {1, 2});
        rectangle("plain_z", Z, box(2, 3, 0, 0), new int[] {0, 0});
        rectangle("wide", X, box(6, 4, -2, 0), new int[] {0, 1});
        rectangle("stacked", X, join(box(2, 3, 0, 0), box(4, 2, -1, 3)), new int[] {0, 0});
        rectangle("stacked_top", X, join(box(2, 3, 0, 0), box(4, 2, -1, 3)), new int[] {2, 4});
        rectangle("stepped", Z, join(join(box(1, 5, 0, 0), box(1, 3, 1, 1)), box(3, 1, 2, 2)), new int[] {1, 2});
        rectangle("huge", X, box(23, 23, 0, 0), new int[] {3, 4});
    }
}
