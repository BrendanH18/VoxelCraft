"""Independent models of Java 26.3's core workstations, for fixture extractors.

Written from the decompiled 26.3 classes (`AbstractFurnaceBlockEntity`,
`BrewingStandBlockEntity`, `EnchantmentMenu`, `EnchantmentHelper`,
`AnvilMenu`, `GrindstoneMenu`, `FallingBlockEntity`,
`AbstractContainerMenu.getRedstoneSignalFromContainer`,
`LegacyRandomSource`/`BitRandomSource`), in Java's int, float and random
arithmetic. The tables they read (recipes, fuels, enchantment definitions,
tags, durabilities) are passed in from the pinned jar by
`extract_java_workstation_fixtures.py`; nothing here reads VoxelCraft.
"""

import math
import struct

MULTIPLIER = 0x5DEECE66D
MASK = (1 << 48) - 1
INT_MAX = (1 << 31) - 1


def f32(x):
    """Round to a Java float."""
    return struct.unpack("f", struct.pack("f", x))[0]


def i32(x):
    """Wrap to a Java int."""
    x &= 0xFFFFFFFF
    return x - (1 << 32) if x > INT_MAX else x


class JavaRandom:
    """`LegacyRandomSource` with `BitRandomSource`'s derived draws."""

    def __init__(self, seed):
        self.seed = (seed ^ MULTIPLIER) & MASK

    def next(self, bits):
        self.seed = (self.seed * MULTIPLIER + 0xB) & MASK
        return i32(self.seed >> (48 - bits))

    def next_int(self, bound):
        if bound <= 0:
            raise ValueError("bound must be positive")
        if bound & -bound == bound:
            return (bound * self.next(31)) >> 31
        while True:
            bits = self.next(31)
            value = bits % bound
            if bits - value + (bound - 1) <= INT_MAX:
                return value

    def next_float(self):
        return f32(self.next(24) * 5.9604645e-8)


def java_round(x):
    """`Math.round(float)`: the nearest int, halves up, computed exactly."""
    return math.floor(x + 0.5)


# Enchanting ---------------------------------------------------------------


def enchantment_cost(rng, slot, bookcases, enchantable):
    """`EnchantmentHelper.getEnchantmentCost`."""
    if enchantable is None:
        return 0
    bookcases = min(bookcases, 15)
    selected = rng.next_int(8) + 1 + (bookcases >> 1) + rng.next_int(bookcases + 1)
    if slot == 0:
        return max(selected // 3, 1)
    return selected * 2 // 3 + 1 if slot == 1 else max(selected, bookcases * 2)


def level_cost(cost, level):
    """`Enchantment.Cost.calculate`."""
    return cost["base"] + cost["per_level_above_first"] * (level - 1)


def compatible(defs, a, b):
    """`Enchantment.areCompatible`."""
    return a != b and b not in defs[a]["exclusive"] and a not in defs[b]["exclusive"]


def weighted(rng, entries, defs):
    """`WeightedRandom.getRandomItem` over `(enchantment, level)` entries."""
    total = sum(defs[e]["weight"] for e, _ in entries)
    if total == 0:
        return None
    index = rng.next_int(total)
    for entry in entries:
        index -= defs[entry[0]]["weight"]
        if index < 0:
            return entry
    return None


def available(value, item, source, defs):
    """`getAvailableEnchantmentResults`: the best level of each enchantment."""
    out = []
    for e in source:
        d = defs[e]
        if not (item == "book" or item in d["primary"]):
            continue
        for level in range(d["max_level"], 0, -1):
            if level_cost(d["min_cost"], level) <= value <= level_cost(d["max_cost"], level):
                out.append((e, level))
                break
    return out


def select_enchantment(rng, item, enchantable, cost, source, defs):
    """`EnchantmentHelper.selectEnchantment`."""
    results = []
    if enchantable is None:
        return results
    cost += 1 + rng.next_int(enchantable // 4 + 1) + rng.next_int(enchantable // 4 + 1)
    span = f32(f32(f32(rng.next_float() + rng.next_float()) - 1.0) * f32(0.15))
    cost = max(java_round(f32(cost + f32(cost * span))), 1)
    entries = available(cost, item, source, defs)
    if entries:
        first = weighted(rng, entries, defs)
        if first:
            results.append(first)
        while rng.next_int(50) <= cost:
            if results:
                last = results[-1][0]
                entries = [x for x in entries if compatible(defs, last, x[0])]
            if not entries:
                break
            pick = weighted(rng, entries, defs)
            if pick:
                results.append(pick)
            cost //= 2
    return results


def enchantment_list(seed, item, enchantable, slot, cost, source, defs, rng=None):
    """`EnchantmentMenu.getEnchantmentList`, reseeding `rng` with seed + slot."""
    rng = rng or JavaRandom(0)
    rng.__init__(i32(seed + slot))
    out = select_enchantment(rng, item, enchantable, cost, source, defs)
    if item == "book" and len(out) > 1:
        out.pop(rng.next_int(len(out)))
    return out, rng


def table_offers(seed, item, enchantable, bookcases, source, defs):
    """`EnchantmentMenu.slotsChanged`: costs, clues and each offer's roll."""
    rng = JavaRandom(seed)
    costs = []
    for slot in range(3):
        c = enchantment_cost(rng, slot, bookcases, enchantable)
        costs.append(0 if c < slot + 1 else c)
    clues, lists = [], []
    for slot in range(3):
        if costs[slot] > 0:
            entries, rng = enchantment_list(seed, item, enchantable, slot, costs[slot], source, defs, rng)
            clues.append(list(entries[rng.next_int(len(entries))]) if entries else None)
            lists.append([list(e) for e in entries])
        else:
            clues.append(None)
            lists.append([])
    return {"costs": costs, "clues": clues, "enchantments": lists}


def bookshelf_offsets():
    """`EnchantingTableBlock.BOOKSHELF_OFFSETS`: `betweenClosed(-2, 0, -2,
    2, 1, 2)` on the ring two out, in `betweenClosed`'s x, then z... order
    is irrelevant here; each with the cell `isValidBookShelf` checks."""
    out = []
    for y in range(0, 2):
        for z in range(-2, 3):
            for x in range(-2, 3):
                if abs(x) == 2 or abs(z) == 2:
                    # Java's int division truncates toward zero.
                    out.append({"offset": [x, y, z], "between": [int(x / 2), y, int(z / 2)]})
    return out


# Anvil and grindstone -----------------------------------------------------


def anvil_result(left, right, name, creative, items, defs):
    """`AnvilMenu.createResult` plus `mayPickup`'s cost: stacks are dicts
    `{item, count, damage, enchantments: {id: level}, repair_cost, name}`;
    `items[id]` gives `max_damage` (None if it doesn't wear) and `repairs`
    (the items that repair it). Returns the result, its cost and how many
    of `right` it uses (None: the whole stack; 0: none, a rename alone)."""
    out = dict(left, enchantments=dict(left["enchantments"]))
    tax = left["repair_cost"] + (right["repair_cost"] if right else 0)
    price = 0
    uses = None
    if right:
        book = right["item"] == "enchanted_book"
        max_damage = items[left["item"]]["max_damage"]
        if max_damage is not None and right["item"] in items[left["item"]]["repairs"]:
            amount = min(out["damage"], max_damage // 4)
            if amount <= 0:
                return None
            count = 0
            while amount > 0 and count < right["count"]:
                out["damage"] -= amount
                price += 1
                amount = min(out["damage"], max_damage // 4)
                count += 1
            uses = count
        else:
            if not book and (left["item"] != right["item"] or max_damage is None):
                return None
            if max_damage is not None and not book:
                remaining = (max_damage - left["damage"]) + (max_damage - right["damage"]) + max_damage * 12 // 100
                damage = max(max_damage - remaining, 0)
                if damage < out["damage"]:
                    out["damage"] = damage
                    price += 2
            fit = clash = False
            for e, level in right["enchantments"].items():
                current = out["enchantments"].get(e, 0)
                level = level + 1 if current == level else max(level, current)
                ok = left["item"] in defs[e]["supported"] or creative or left["item"] == "enchanted_book"
                for other in list(out["enchantments"]):
                    if other != e and not compatible(defs, e, other):
                        ok = False
                        price += 1
                if not ok:
                    clash = True
                    continue
                fit = True
                level = min(level, defs[e]["max_level"])
                out["enchantments"][e] = level
                fee = defs[e]["anvil_cost"]
                if book:
                    fee = max(1, fee // 2)
                price += fee * level
                if left["count"] > 1:
                    price = 40
            if clash and not fit:
                return None
    naming = 0
    if name is not None and name.strip():
        if name != left["name"]:
            naming = 1
            price += naming
            out["name"] = name
    elif name is not None and left["name"] is not None:
        naming = 1
        price += naming
        out["name"] = None
    if price <= 0:
        return None
    cost = min(tax + price, INT_MAX)
    only_renaming = naming == price and naming > 0
    if only_renaming:
        cost = min(cost, 39)
        uses = 0
    base = max(left["repair_cost"], right["repair_cost"] if right else 0)
    if not only_renaming:
        base = min(base * 2 + 1, INT_MAX)
    out["repair_cost"] = base
    return {"output": out, "cost": cost, "uses": uses, "too_expensive": cost >= 40 and not creative}


def grindstone_result(a, b, items, curses):
    """`GrindstoneMenu.computeResult` and the result slot's experience
    range (`half + nextInt(half)`, from `getExperienceAmount`)."""
    if a is None and b is None:
        return None
    if (a and a["count"] > 1) or (b and b["count"] > 1):
        return None
    if a and b:
        if a["item"] != b["item"]:
            return None
        info = items[a["item"]]
        count = 1
        out = dict(a, enchantments=dict(a["enchantments"]))
        if info["max_damage"] is None:
            same = all(a[k] == b[k] for k in ("enchantments", "repair_cost", "name", "damage"))
            if info["max_stack"] < 2 or not same:
                return None
            count = 2
        else:
            durability = info["max_damage"]
            remaining = (durability - a["damage"]) + (durability - b["damage"]) + durability * 5 // 100
            out["damage"] = max(durability - remaining, 0)
        out["count"] = count
        for e, level in b["enchantments"].items():
            if e not in curses or out["enchantments"].get(e, 0) == 0:
                out["enchantments"][e] = max(out["enchantments"].get(e, 0), level)
    else:
        item = a or b
        if not item["enchantments"]:
            return None
        out = dict(item, enchantments=dict(item["enchantments"]))
    out["enchantments"] = {e: lvl for e, lvl in out["enchantments"].items() if e in curses}
    if out["item"] == "enchanted_book" and not out["enchantments"]:
        out["item"] = "book"
    cost = 0
    for _ in out["enchantments"]:
        cost = min(cost * 2 + 1, INT_MAX)
    out["repair_cost"] = cost
    return out


def grindstone_xp(a, b, defs, curses):
    amount = 0
    for stack in (a, b):
        for e, level in (stack or {"enchantments": {}})["enchantments"].items():
            if e not in curses:
                amount += level_cost(defs[e]["min_cost"], level)
    if amount <= 0:
        return [0, 0]
    half = math.ceil(amount / 2.0)
    return [half, half + half - 1]


def falling_anvil(distance):
    """`FallingBlockEntity.causeFallDamage` with an anvil's `(2.0F, 40)`:
    the damage and the chance it lands one step more worn."""
    blocks = math.ceil(distance - 1.0)
    if blocks < 0:
        return {"distance": distance, "damage": 0.0, "wear_chance": 0.0}
    damage = float(min(math.floor(f32(blocks * 2.0)), 40))
    chance = f32(f32(0.05) + f32(blocks * f32(0.05))) if damage > 0.0 else 0.0
    return {"distance": distance, "damage": damage, "wear_chance": chance}


# Furnace and brewing stand -------------------------------------------------


def create_experience(amount, value):
    """`createExperience`: the whole part and the fraction that rounds up."""
    value = f32(value)
    total = f32(amount * value)
    return {"count": amount, "xp": value, "whole": math.floor(total), "fraction": f32(total - math.floor(total))}


class Furnace:
    """`AbstractFurnaceBlockEntity` for one block. `recipes[input]` gives
    `(recipe id, result, xp, cooking time)`; `fuels[item]` the burn ticks
    in this block; `speed(item)` its speed multiplier (1 for non-fuel);
    `max_stack[item]` the stack sizes; `remainders[item]` crafting
    remainders."""

    def __init__(self, recipes, fuels, speed, max_stack, remainders):
        self.recipes, self.fuels, self.speed_of = recipes, fuels, speed
        self.max_stack, self.remainders = max_stack, remainders
        self.input = self.fuel = self.output = None
        self.lit = self.lit_total = self.cook = self.cook_total = 0
        self.speed = 1.0
        self.used = {}
        self.dropped = []

    def total(self, recipe):
        return math.ceil(f32(recipe[3] / self.speed)) if self.speed > 0.0 else recipe[3]

    def set_input(self, stack):
        """`setItem(0, stack)`."""
        same = stack is not None and self.input is not None and self.input[0] == stack[0]
        self.input = stack
        if not same:
            recipe = self.recipes.get(stack[0]) if stack else None
            self.cook_total = self.total(recipe) if recipe else 200
            self.cook = 0

    def tick(self):
        is_lit = False
        if self.lit > 0:
            self.lit -= 1
            is_lit = self.lit > 0
        has_input, has_fuel = self.input is not None, self.fuel is not None
        if is_lit or (has_fuel and has_input):
            if has_input:
                recipe = self.recipes.get(self.input[0])
                if recipe:
                    result = recipe[1]
                    fits = self.output is None or (
                        self.output[0] == result and self.output[1] + 1 <= min(99, self.max_stack[result])
                    )
                    if fits:
                        if not is_lit:
                            fuel = self.fuel[0] if self.fuel else None
                            new_lit = self.fuels.get(fuel, 0)
                            self.lit = self.lit_total = new_lit
                            self.speed = self.speed_of(fuel)
                            if 0 < self.cook_total and self.cook < self.cook_total:
                                ratio = f32(self.cook / self.cook_total)
                                self.cook_total = self.total(recipe)
                                self.cook = math.ceil(f32(ratio * self.cook_total))
                            if new_lit > 0:
                                self.consume_fuel()
                                is_lit = True
                        if is_lit:
                            self.cook += 1
                            if self.cook >= self.cook_total:
                                self.cook = 0
                                self.cook_total = self.total(recipe)
                                self.burn(result)
                                self.used[recipe[0]] = self.used.get(recipe[0], 0) + 1
                        else:
                            self.cook = 0
                    else:
                        self.cook = 0
            else:
                self.cook = 0
        elif self.cook > 0:
            self.cook = min(max(self.cook - 2, 0), self.cook_total)

    def consume_fuel(self):
        item, count = self.fuel
        remainder = self.remainders.get(item)
        self.fuel = (item, count - 1) if count > 1 else None
        if remainder:
            if self.fuel is None:
                self.fuel = (remainder, 1)
            else:
                self.dropped.append(remainder)

    def burn(self, result):
        self.output = (result, 1) if self.output is None else (result, self.output[1] + 1)
        if self.input[0] == "wet_sponge" and self.fuel and self.fuel[0] == "bucket":
            self.fuel = ("water_bucket", 1)
        item, count = self.input
        self.input = (item, count - 1) if count > 1 else None

    def state(self):
        def stack(s):
            return list(s) if s else None

        return {
            "lit": self.lit,
            "lit_total": self.lit_total,
            "cook": self.cook,
            "cook_total": self.cook_total,
            "speed": self.speed,
            "input": stack(self.input),
            "fuel": stack(self.fuel),
            "output": stack(self.output),
            "recipes_used": dict(sorted(self.used.items())),
        }


class BrewingStand:
    """`BrewingStandBlockEntity`. Bottles are `(item, potion)`;
    `mixes[(item, potion, reagent)]` gives the result bottle; `reagents` is
    `BREWING_REAGENTS`; `fuel_uses[item]` the brewing fuels."""

    def __init__(self, mixes, reagents, fuel_uses, remainders):
        self.mixes, self.reagents, self.fuel_uses, self.remainders = mixes, reagents, fuel_uses, remainders
        self.bottles = [None, None, None]
        self.ingredient = self.fuel_slot = None
        self.brew_time = self.fuel = 0
        self.brewing = None
        self.brews = 0
        self.dropped = []

    def brewable(self):
        if self.ingredient is None or self.ingredient[0] not in self.reagents:
            return False
        return any(b and (b[0], b[1], self.ingredient[0]) in self.mixes for b in self.bottles)

    def tick(self):
        if self.fuel <= 0 and self.fuel_slot and self.fuel_slot[0] in self.fuel_uses:
            self.fuel = self.fuel_uses[self.fuel_slot[0]]
            item, count = self.fuel_slot
            self.fuel_slot = (item, count - 1) if count > 1 else None
        brewable = self.brewable()
        if self.brew_time > 0:
            self.brew_time -= 1
            if self.brew_time == 0 and brewable:
                self.brew()
            elif not brewable or self.ingredient is None or self.ingredient[0] != self.brewing:
                self.brew_time = 0
        elif brewable and self.fuel > 0:
            self.fuel -= 1
            self.brew_time = math.ceil(f32(400.0 / 1.0))
            self.brewing = self.ingredient[0]

    def brew(self):
        reagent = self.ingredient[0]
        for i, b in enumerate(self.bottles):
            if b and (b[0], b[1], reagent) in self.mixes:
                self.bottles[i] = self.mixes[(b[0], b[1], reagent)]
        remainder = self.remainders.get(reagent)
        item, count = self.ingredient
        self.ingredient = (item, count - 1) if count > 1 else None
        if remainder:
            if self.ingredient is None:
                self.ingredient = (remainder, 1)
            else:
                self.dropped.append(remainder)
        self.brews += 1

    def state(self):
        return {
            "brew_time": self.brew_time,
            "fuel": self.fuel,
            "bottles": [list(b) if b else None for b in self.bottles],
            "ingredient": list(self.ingredient) if self.ingredient else None,
            "fuel_slot": list(self.fuel_slot) if self.fuel_slot else None,
            "brews": self.brews,
            "dropped": list(self.dropped),
        }


def comparator_signal(stacks, max_stack):
    """`getRedstoneSignalFromContainer` over a container's slots."""
    total = 0.0
    for s in stacks:
        if s:
            total = f32(total + f32(s[1] / min(99, max_stack[s[0]])))
    total = f32(total / len(stacks))
    return math.floor(f32(total * 14)) + (1 if total > 0.0 else 0)
