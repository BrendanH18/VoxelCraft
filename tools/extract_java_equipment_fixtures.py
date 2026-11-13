#!/usr/bin/env python3
"""Extract armor equipment facts from the pinned official Java 26.3 artifact.

Requires javap. Records every `ArmorMaterials` material, the `ArmorType`
durability units, the four `Item$Properties` armor forms (humanoid, wolf,
horse and nautilus: durability, enchantability, repair, stack size and the
`Equippable` component), every item registered with one of those forms in
`Items` (material, armor type, rarity, fire resistance), the entity tags that
say who may wear body armor, and the villager trades that sell armor.
Unknown bytecode forms fail rather than regenerating a guessed fixture.
No VoxelCraft source or existing fixture is consulted.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1

FORMS = {"humanoidArmor": "humanoid", "wolfArmor": "wolf", "horseArmor": "horse", "nautilusArmor": "nautilus"}


def float_operands(code):
    values = []
    for line in code.splitlines():
        match = re.search(r"\d+: (?:fconst_([0-2])|ldc(?:_w)?\s+#\d+\s+// float (-?[\d.]+)f)", line)
        if match:
            values.append(float(match[1] if match[1] is not None else match[2]))
    return values


def int_operands(code):
    values = []
    for line in code.splitlines():
        match = re.search(r"\d+: (?:iconst_([0-5])|bipush\s+(-?\d+)|sipush\s+(-?\d+))\s*$", line)
        if match:
            values.append(int(next(g for g in match.groups() if g is not None)))
    return values


def method(code, name):
    match = re.search(r"\n  public [^\n]* " + name + r"\([^\n]*\);\n    Code:\n(.*?)\n\n", code, re.S)
    if not match:
        raise ValueError(f"missing reference method: {name}")
    return match[1]


def boolean_before(code, call):
    match = re.search(r"\d+: iconst_([01])\n[^\n]*Equippable\$Builder\." + call + r":\(Z\)", code)
    return None if match is None else match[1] == "1"


def form_facts(code, form):
    tag = re.search(r"EntityTypeTags\.(\w+):", code)
    direct = re.findall(r"EntityTypes\.(\w+):", code)
    stacks = re.search(r"(?:iconst_(\d)|bipush\s+(\d+))\n[^\n]*Method stacksTo:", code)
    equip_sound = re.search(r"SoundEvents\.(\w+):[^\n]*\n[^\n]*setEquipSound", code)
    shear_sound = re.search(r"SoundEvents\.(\w+):[^\n]*\n(?:[^\n]*\n)?[^\n]*setShearingSound", code)
    if "Equippable.builder" not in code or "createAttributes" not in code:
        raise ValueError(f"unrecognized armor form: {form}")
    slot = re.search(r"EquipmentSlot\.(\w+):[^\n]*\n[^\n]*Equippable\.builder", code)
    return {
        "slot": slot[1].lower() if slot else "armor_type",
        "durability": "Method durability:" in code,
        "enchantable": "Method enchantable:" in code,
        "repairable": "Method repairable:" in code,
        "stacks_to": int(stacks[1] or stacks[2]) if stacks else None,
        "equip_sound": equip_sound[1].lower() if equip_sound else "material",
        "allowed_entities": f"#{tag[1].lower()}" if tag else [e.lower() for e in direct] or None,
        "damage_on_hurt": boolean_before(code, "setDamageOnHurt") is not False,
        "equip_on_interact": boolean_before(code, "setEquipOnInteract") is True,
        "can_be_sheared": boolean_before(code, "setCanBeSheared") is True,
        "shearing_sound": shear_sound[1].lower() if shear_sound else None,
    }


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-equipment-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", name], text=True)

        # ArmorType: slot and durability unit per type.
        types = {}
        block = inspect("net.minecraft.world.item.equipment.ArmorType").split("  static {};")[1]
        for name, slot, units in re.findall(
                r"String ([A-Z]+)\n[^\n]*\n[^\n]*EquipmentSlot\.(\w+):[^\n]*\n[^\n]*bipush\s+(\d+)", block):
            types[name.lower()] = {"slot": slot.lower(), "durability_unit": int(units)}
        if set(types) != {"helmet", "chestplate", "leggings", "boots", "body"}:
            raise ValueError(f"unrecognized armor types: {sorted(types)}")

        # makeDefense's parameter order, read from its body.
        materials_code = inspect("net.minecraft.world.item.equipment.ArmorMaterials")
        make = re.search(r"makeDefense\(int, int, int, int, int\);\n    Code:\n(.*?)\n\n", materials_code, re.S)
        if not make:
            raise ValueError("missing ArmorMaterials.makeDefense")
        order = [None] * 5
        for armor_type, local in re.findall(r"ArmorType\.(\w+):[^\n]*\n[^\n]*iload(?:_|\s+)(\d)", make[1]):
            order[int(local)] = armor_type.lower()
        if sorted(o or "" for o in order) != sorted(types):
            raise ValueError("unrecognized makeDefense parameter order")
        materials = {}
        block = materials_code.split("  static {};")[1]
        pieces = re.split(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):Lnet/minecraft/world/item/equipment/ArmorMaterial;",
                          block)
        for code, name in zip(pieces[0::2], pieces[1::2]):
            ints, floats = int_operands(code), float_operands(code)
            repair = re.search(r"ItemTags\.(\w+):", code)
            sound = re.search(r"SoundEvents\.(\w+):", code)
            asset = re.search(r"EquipmentAssets\.(\w+):", code)
            if "makeDefense" not in code or len(ints) != 7 or len(floats) != 2 or not (repair and sound and asset):
                raise ValueError(f"unrecognized armor material initialization: {name}")
            materials[name.lower()] = {
                "durability_multiplier": ints[0],
                "defense": {order[i]: ints[1 + i] for i in range(5)},
                "enchantability": ints[6],
                "toughness": floats[0],
                "knockback_resistance": floats[1],
                "repair_tag": repair[1].lower(),
                "equip_sound": sound[1].lower(),
                "asset": asset[1].lower(),
            }

        properties = inspect("net.minecraft.world.item.Item$Properties")
        forms = {label: form_facts(method(properties, name), label) for name, label in FORMS.items()}

        # Item ids: ItemIds holds each registry key's string.
        ids = {}
        block = inspect("net.minecraft.references.ItemIds").split("  static {};")[1]
        for key, field in re.findall(r"// String ([a-z0-9_]+)\n[^\n]*Method create:[^\n]*\n\s+\d+: putstatic[^\n]*// Field ([A-Z0-9_]+):",
                                     block):
            ids[field] = key
        items = {}
        block = inspect("net.minecraft.world.item.Items").split("  static {};")[1]
        pieces = re.split(r"putstatic\s+#\d+\s+// Field ([A-Z0-9_]+):Lnet/minecraft/world/item/Item;", block)
        for code, field in zip(pieces[0::2], pieces[1::2]):
            form = re.search(r"Item\$Properties\.(" + "|".join(FORMS) + r"):", code)
            if not form:
                continue
            key = re.search(r"ItemIds\.([A-Z0-9_]+):", code)
            material = re.findall(r"ArmorMaterials\.([A-Z_]+):", code)
            armor_type = re.findall(r"ArmorType\.([A-Z]+):", code)
            rarity = re.search(r"Rarity\.([A-Z]+):", code)
            if not key or key[1] not in ids or len(material) != 1:
                raise ValueError(f"unrecognized armor item registration: {field}")
            label = FORMS[form[1]]
            if label == "humanoid":
                if len(armor_type) != 1:
                    raise ValueError(f"unrecognized humanoid armor type: {field}")
                piece = armor_type[0].lower()
            else:
                if armor_type:
                    raise ValueError(f"unexpected armor type on body armor: {field}")
                piece = "body"
            items[ids[key[1]]] = {
                "form": label,
                "material": material[0].lower(),
                "type": piece,
                "rarity": rarity[1].lower() if rarity else "common",
                "fire_resistant": "Item$Properties.fireResistant:" in code,
            }
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        for facts in materials.values():
            facts["repair_items"] = read(f"data/minecraft/tags/item/{facts['repair_tag']}.json")["values"]
        tags = {}
        for facts in forms.values():
            allowed = facts["allowed_entities"]
            if isinstance(allowed, str):
                name = allowed[1:]
                tags[name] = read(f"data/minecraft/tags/entity_type/{name}.json")["values"]
        trades = []
        for name in sorted(jar.namelist()):
            match = re.fullmatch(r"data/minecraft/villager_trade/(\w+)/(\d)/(\w+)\.json", name)
            if not match:
                continue
            trade = read(name)
            gives = trade["gives"]["id"].removeprefix("minecraft:")
            if items.get(gives, {}).get("form") is None:
                continue
            wants = trade["wants"]
            trades.append({"path": name, "profession": match[1], "level": int(match[2]), "id": match[3],
                           "wants": {"id": wants["id"].removeprefix("minecraft:"), "count": wants.get("count", 1)},
                           "gives": {"id": gives, "count": trade["gives"].get("count", 1)},
                           # VillagerTrade's codec defaults: 4 uses, 1 xp, no discount.
                           "enchanted": "additional_wants" in trade or "given_item_modifier" in trade,
                           "max_uses": trade.get("max_uses", 4), "xp": trade.get("xp", 1),
                           "reputation_discount": trade.get("reputation_discount", 0.0)})
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.item.equipment.ArmorType",
            "net.minecraft.world.item.equipment.ArmorMaterials (static initializer and makeDefense)",
            "net.minecraft.world.item.Item$Properties.humanoidArmor/wolfArmor/horseArmor/nautilusArmor",
            "net.minecraft.references.ItemIds",
            "net.minecraft.world.item.Items (static initializer)",
            "data/minecraft/tags/item/repairs_*.json",
            "data/minecraft/tags/entity_type/can_wear_*_armor.json",
            "data/minecraft/villager_trade/*/*/*.json (offers that give armor)",
        ],
        "armor_types": types,
        "materials": dict(sorted(materials.items())),
        "forms": forms,
        "items": dict(sorted(items.items())),
        "entity_tags": tags,
        "trades": trades,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n")


if __name__ == "__main__":
    main()
