// Independent JDK oracle: original code, behaviour data only, no game assets.
import java.util.*;
import java.security.*;
import java.nio.charset.StandardCharsets;
class LootVectors {
    record S(String name, int count) {}
    static int between(Random r, int lo, int hi) { return lo >= hi ? lo : lo + r.nextInt(hi-lo+1); }
    static String fill(Random r, ArrayList<S> items, int size) {
        ArrayList<Integer> slots = new ArrayList<>();
        for (int i=0; i<size; i++) slots.add(i);
        Collections.shuffle(slots, r);
        ArrayList<S> ready = new ArrayList<>(), split = new ArrayList<>();
        for (S s: items) { if (s.count>1) split.add(s); else if (s.count>0) ready.add(s); }
        while (slots.size()-ready.size()-split.size()>0 && !split.isEmpty()) {
            S s=split.remove(between(r,0,split.size()-1));
            int count=between(r,1,s.count/2);
            S left=new S(s.name,s.count-count), copy=new S(s.name,count);
            if (left.count>1 && r.nextBoolean()) split.add(left); else ready.add(left);
            if (copy.count>1 && r.nextBoolean()) split.add(copy); else ready.add(copy);
        }
        ready.addAll(split); Collections.shuffle(ready,r);
        String[] chest=new String[size]; Arrays.fill(chest,"-");
        for (S s: ready) chest[slots.remove(slots.size()-1)]=s.name+":"+s.count;
        return Arrays.toString(chest);
    }
    static void cartographer(long seed) {
        Random r=new Random(seed); ArrayList<S> items=new ArrayList<>();
        int rolls=between(r,1,5);
        String[] names={"map","paper","compass","bread","stick"};
        int[] weights={10,15,5,15,5}, maximum={3,5,1,4,2};
        for (int i=0; i<rolls; i++) {
            int w=r.nextInt(50);
            for (int j=0; j<weights.length; j++) {
                if (w<weights[j]) { items.add(new S(names[j],between(r,1,maximum[j]))); break; }
                w-=weights[j];
            }
        }
        // The bundle-or-empty pool: bundle weight 1, empty weight 2.
        if (r.nextInt(3) < 1) items.add(new S("bundle",1));
        System.out.println(seed+" "+fill(r,items,27));
    }
    static long mix(long z) { z=(z^(z>>>30))*0xbf58476d1ce4e5b9L; z=(z^(z>>>27))*0x94d049bb133111ebL; return z^(z>>>31); }
    static long be(byte[] b,int offset) { long out=0; for(int i=offset;i<offset+8;i++) out=(out<<8)|(b[i]&255L); return out; }
    public static void main(String[] args) throws Exception {
        for (long seed: new long[]{0,42,12345,1}) cartographer(seed);
        System.out.println("split "+fill(new Random(42),new ArrayList<>(List.of(new S("arrow",14),new S("paper",3),new S("diamond",1))),9));
        byte[] h=MessageDigest.getInstance("MD5").digest("minecraft:chests/shipwreck_map".getBytes(StandardCharsets.UTF_8));
        long lo=42^0x6a09e667f3bcc909L,hi=lo+0x9e3779b97f4a7c15L;
        lo=mix(lo^be(h,0)); hi=mix(hi^be(h,8));
        System.out.println("state "+Long.toUnsignedString(lo)+" "+Long.toUnsignedString(hi));
        for(int i=0;i<4;i++) { long a=lo,b=hi,result=Long.rotateLeft(a+b,17)+a; b^=a; lo=Long.rotateLeft(a,49)^b^(b<<21); hi=Long.rotateLeft(b,28); System.out.println("next "+Long.toUnsignedString(result)); }
    }
}
