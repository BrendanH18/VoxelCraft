// v1.0 gap Track 7c: `NautilusAnimation`'s keyframes from the Java 26.3 client (numbers only),
// turned into tables by the record's converter. Included into the model's module,
// which names the bones.
pub(super) const SWIMMING: Anim = Anim {
    length: 1.0,
    looping: true,
    channels: &[
        ch(BODY, Scale, &[l(0.0, [1.0, 1.0, 1.0]), l(0.5, [1.0, 1.0, 1.2]), l(0.75, [1.0, 1.0, 0.9]), l(0.875, [1.0, 1.0, 1.0]), l(1.0, [1.0, 1.0, 1.0])]),
        ch(UPPER_MOUTH, Rot, &[l(0.0, [0.0, 0.0, 0.0]), l(0.5, [30.0, 0.0, 0.0]), l(0.75, [0.0, 0.0, 0.0]), l(0.875, [0.0, 0.0, 0.0]), l(1.0, [0.0, 0.0, 0.0])]),
        ch(UPPER_MOUTH, Scale, &[l(0.0, [1.0, 1.0, 1.0]), l(0.5, [1.0, 1.0, 1.4]), l(0.75, [1.0, 1.0, 0.9]), l(0.875, [1.0, 1.0, 1.0]), l(1.0, [1.0, 1.0, 1.0])]),
        ch(INNER_MOUTH, Scale, &[l(0.0, [1.0, 1.0, 1.0]), l(0.5, [0.8, 0.8, 1.0]), l(0.75, [1.0, 1.0, 0.9]), l(0.875, [1.0, 1.0, 1.0]), l(1.0, [1.0, 1.0, 1.0])]),
        ch(LOWER_MOUTH, Rot, &[l(0.0, [0.0, 0.0, 0.0]), l(0.5, [-30.0, 0.0, 0.0]), l(0.75, [0.0, 0.0, 0.0]), l(0.875, [0.0, 0.0, 0.0]), l(1.0, [0.0, 0.0, 0.0])]),
        ch(LOWER_MOUTH, Scale, &[l(0.0, [1.0, 1.0, 1.0]), l(0.5, [1.0, 1.0, 1.4]), l(0.75, [1.0, 1.0, 0.9]), l(0.875, [1.0, 1.0, 1.0]), l(1.0, [1.0, 1.0, 1.0])]),
    ],
};
