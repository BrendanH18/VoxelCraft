// Layout vectors for VoxelCraft's generation 5 piece-by-piece structures,
// read by running the official Java 26.3 server's own piece generators
// (tools/extract_java_classic_structure_fixtures.py compiles and runs this
// against the verified jar). Original code: it only calls the jar's public
// API and prints what the pieces are; nothing of the game is copied.
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.InfestedBlock;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.WorldgenRandom;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePiecesBuilder;
import net.minecraft.world.level.levelgen.structure.structures.MineshaftPieces;
import net.minecraft.world.level.levelgen.structure.structures.MineshaftStructure;
import net.minecraft.world.level.levelgen.structure.structures.NetherFortressPieces;
import net.minecraft.world.level.levelgen.structure.structures.StrongholdPieces;

public class JavaStructureVectors {
    static WorldgenRandom random(long seed, int cx, int cz) {
        WorldgenRandom r = new WorldgenRandom(new LegacyRandomSource(0L));
        r.setLargeFeatureSeed(seed, cx, cz);
        return r;
    }

    static void print(String structure, long seed, int cx, int cz, List<StructurePiece> pieces) {
        StringBuilder out = new StringBuilder();
        out.append(structure).append(' ').append(seed).append(' ').append(cx).append(' ').append(cz);
        for (StructurePiece p : pieces) {
            BoundingBox b = p.getBoundingBox();
            out.append(" | ").append(p.getClass().getSimpleName()).append(' ').append(p.getOrientation()).append(' ')
                .append(p.getGenDepth()).append(' ').append(b.minX()).append(' ').append(b.minY()).append(' ')
                .append(b.minZ()).append(' ').append(b.maxX()).append(' ').append(b.maxY()).append(' ').append(b.maxZ());
        }
        System.out.println(out);
    }

    static void fortress(long seed, int cx, int cz) {
        WorldgenRandom random = random(seed, cx, cz);
        StructurePiecesBuilder builder = new StructurePiecesBuilder();
        NetherFortressPieces.StartPiece start = new NetherFortressPieces.StartPiece(random, cx * 16 + 2, cz * 16 + 2);
        builder.addPiece(start);
        start.addChildren(start, builder, random);
        List<StructurePiece> pending = start.pendingChildren;
        while (!pending.isEmpty()) {
            pending.remove(random.nextInt(pending.size())).addChildren(start, builder, random);
        }
        builder.moveInsideHeights(random, 48, 70);
        print("fortress", seed, cx, cz, builder.build().pieces());
    }

    static void stronghold(long seed, int cx, int cz) {
        StructurePiecesBuilder builder = new StructurePiecesBuilder();
        WorldgenRandom random = new WorldgenRandom(new LegacyRandomSource(0L));
        int tries = 0;
        StrongholdPieces.StartPiece start;
        do {
            builder.clear();
            random.setLargeFeatureSeed(seed + tries++, cx, cz);
            StrongholdPieces.resetPieces();
            start = new StrongholdPieces.StartPiece(random, cx * 16 + 2, cz * 16 + 2);
            builder.addPiece(start);
            start.addChildren(start, builder, random);
            List<StructurePiece> pending = start.pendingChildren;
            while (!pending.isEmpty()) {
                pending.remove(random.nextInt(pending.size())).addChildren(start, builder, random);
            }
            builder.moveBelowSeaLevel(63, -64, random, 10);
        } while (builder.isEmpty() || start.portalRoomPiece == null);
        print("stronghold", seed, cx, cz, builder.build().pieces());
    }

    // The mesa type's height: `randomBetweenInclusive(seaLevel, surface)` with
    // the surface given as `surface`.
    static void mineshaft(long seed, int cx, int cz, boolean mesa, int surface) {
        WorldgenRandom random = random(seed, cx, cz);
        random.nextDouble();
        StructurePiecesBuilder builder = new StructurePiecesBuilder();
        MineshaftStructure.Type type = mesa ? MineshaftStructure.Type.MESA : MineshaftStructure.Type.NORMAL;
        MineshaftPieces.MineShaftRoom room = new MineshaftPieces.MineShaftRoom(0, random, cx * 16 + 2, cz * 16 + 2, type);
        builder.addPiece(room);
        room.addChildren(room, builder, random);
        if (mesa) {
            int centre = builder.getBoundingBox().getCenter().getY();
            int target = surface <= 63 ? 63 : random.nextInt(surface - 63 + 1) + 63;
            builder.offsetPiecesVertically(target - centre);
        } else {
            builder.moveBelowSeaLevel(63, -64, random, 10);
        }
        print(mesa ? "mineshaft_mesa" : "mineshaft", seed, cx, cz, builder.build().pieces());
    }

    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        for (Block b : new Block[] {Blocks.INFESTED_STONE, Blocks.INFESTED_COBBLESTONE, Blocks.INFESTED_STONE_BRICKS,
                Blocks.INFESTED_MOSSY_STONE_BRICKS, Blocks.INFESTED_CRACKED_STONE_BRICKS,
                Blocks.INFESTED_CHISELED_STONE_BRICKS, Blocks.INFESTED_DEEPSLATE}) {
            InfestedBlock i = (InfestedBlock) b;
            System.out.println("infested " + b.builtInRegistryHolder().key().identifier().getPath() + " "
                + i.getHostBlock().builtInRegistryHolder().key().identifier().getPath() + " "
                + b.defaultDestroyTime() + " " + b.getExplosionResistance());
        }
        long[] seeds = {42L, -7311026468235466081L};
        int[][] chunks = {{0, 0}, {3, 5}, {-12, 7}};
        for (long seed : seeds) {
            for (int[] c : chunks) {
                fortress(seed, c[0], c[1]);
                stronghold(seed, c[0], c[1]);
                mineshaft(seed, c[0], c[1], false, 0);
                mineshaft(seed, c[0], c[1], true, 90);
            }
        }
    }
}
