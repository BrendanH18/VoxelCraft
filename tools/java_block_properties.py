"""Evaluate Java 26.3 block registrations symbolically from `javap` output.

`Blocks.<clinit>` builds each block's `BlockBehaviour.Properties` with a chain
of calls (`strength`, `requiresCorrectToolForDrops`, copies of other blocks,
helper methods and colour or copper collection lambdas). This module runs that
bytecode on a small symbolic stack machine and records, for every registered
block ID, the three properties the mining fixture needs: `destroyTime`,
`explosionResistance` and `requiresCorrectToolForDrops`. Every other value is
opaque. Unknown opcodes, unknown collection helpers, registrations inside a
branch and branches whose outcomes differ fail instead of guessing.

Used by fixture extractors only. Never reads VoxelCraft source.
"""
import copy
import re
import subprocess

BLOCKS = "net/minecraft/world/level/block/Blocks"
BLOCK_ITEM_IDS = "net/minecraft/references/BlockItemIds"
BLOCK_IDS = "net/minecraft/references/BlockIds"
ITEM_IDS = "net/minecraft/references/ItemIds"
REGISTRATION_CLASSES = (BLOCKS, BLOCK_ITEM_IDS, BLOCK_IDS, ITEM_IDS)
PROPERTIES = "net/minecraft/world/level/block/state/BlockBehaviour$Properties"
COLOR_COLLECTION = "net/minecraft/world/level/block/ColorCollection"
COPPER_COLLECTION = "net/minecraft/world/level/block/WeatheringCopperCollection"
BY_STATE = "net/minecraft/world/level/block/WeatheringCopperCollection$ByState"
DYE_COLOR = "net/minecraft/world/item/DyeColor"
WEATHER_STATE = "net/minecraft/world/level/block/WeatheringCopper$WeatherState"
FUNCTIONAL = ("java/util/function/Function", "java/util/function/BiFunction",
              "org/apache/commons/lang3/function/TriFunction", "java/util/function/Supplier")


DEAD = object()


class Opaque:
    """A value the mining properties don't depend on."""

    def __init__(self, what):
        self.what = what

    def __repr__(self):
        return f"Opaque({self.what})"

    def __eq__(self, other):
        return isinstance(other, Opaque)

    def __hash__(self):
        return 0


class Props:
    def __init__(self, destroy=0.0, resistance=0.0, requires=False):
        self.destroy, self.resistance, self.requires = destroy, resistance, requires

    def key(self):
        return (self.destroy, self.resistance, self.requires)

    def copy(self):
        return Props(*self.key())

    def __eq__(self, other):
        return isinstance(other, Props) and self.key() == other.key()

    def __repr__(self):
        return f"Props{self.key()}"


class Key:
    """A registry ID (`BlockItemId`, `ResourceKey<Block>` or `ResourceKey<Item>`).
    A `BlockItemId` can name its item differently (`pumpkin_stem` and
    `pumpkin_seeds`); `name` is `None` when it isn't a plain string."""

    def __init__(self, name, item=None):
        self.name, self.item = name, item or name

    def __eq__(self, other):
        return isinstance(other, Key) and other.name == self.name

    def __repr__(self):
        return f"Key({self.name})"


class Enum:
    def __init__(self, kind, name):
        self.kind, self.name = kind, name

    def __eq__(self, other):
        return isinstance(other, Enum) and (self.kind, self.name) == (other.kind, other.name)

    def __hash__(self):
        return hash((self.kind, self.name))

    def __repr__(self):
        return f"{self.kind}.{self.name}"


class Coll:
    """A colour collection (keyed by colour name) or copper collection
    (keyed by `(waxed, state)`), or a copper `ByState` (keyed by state)."""

    def __init__(self, kind, items):
        self.kind, self.items = kind, items

    def map(self, fn):
        return Coll(self.kind, {k: fn(k, v) for k, v in self.items.items()})

    def __eq__(self, other):
        return isinstance(other, Coll) and (self.kind, self.items) == (other.kind, other.items)

    def __repr__(self):
        return f"Coll({self.kind}, {self.items})"


class Lambda:
    def __init__(self, owner, method, descriptor, captured, kind):
        self.owner, self.method, self.descriptor, self.captured, self.kind = owner, method, descriptor, captured, kind

    def __eq__(self, other):
        return isinstance(other, Lambda) and (self.owner, self.method) == (other.owner, other.method)

    def __repr__(self):
        return f"Lambda({self.owner}.{self.method})"


def descriptor_args(descriptor):
    """Parameter type codes of a JVM method descriptor."""
    params = descriptor[descriptor.index("(") + 1:descriptor.index(")")]
    out, i = [], 0
    while i < len(params):
        start = i
        while params[i] == "[":
            i += 1
        if params[i] == "L":
            i = params.index(";", i)
        out.append(params[start:i + 1])
        i += 1
    return out


def descriptor_returns(descriptor):
    return descriptor[descriptor.index(")") + 1:]


JAVA_PRIMITIVES = {"float": "F", "int": "I", "boolean": "Z", "double": "D", "long": "J", "byte": "B",
                   "short": "S", "char": "C"}


def java_type_descriptor(java):
    java = java.strip()
    dims = java.count("[]")
    java = java.replace("[]", "").strip()
    if java.endswith("..."):
        dims += 1
        java = java[:-3]
    code = JAVA_PRIMITIVES.get(java) or "L" + java.replace(".", "/") + ";"
    return "[" * dims + code


def strip_generics(text):
    out, depth = [], 0
    for ch in text:
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif depth == 0:
            out.append(ch)
    return "".join(out)


class ClassCode:
    """Methods of one class from `javap -c -p`, and its lambda bootstrap table."""

    def __init__(self, jar, name):
        self.name = name
        dotted = name.replace("/", ".")
        code = subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", dotted], text=True)
        verbose = subprocess.check_output(["javap", "-classpath", str(jar), "-v", "-p", dotted], text=True)
        self.methods = {}
        for block in code.split("\n\n"):
            lines = block.strip("\n").split("\n")
            header = next((l for l in lines if l.startswith("  ") and not l.startswith("   ")), None)
            if header is None or "Code:" not in block:
                continue
            header = strip_generics(header.strip())
            if header.startswith("static {}"):
                key = ("<clinit>", "()")
            else:
                match = re.search(r"([\w$<>]+)\(([^)]*)\)", header)
                params = [p for p in match[2].split(",") if p.strip()]
                key = (match[1], "(" + "".join(java_type_descriptor(p) for p in params) + ")")
            self.methods[key] = parse_code(lines[lines.index("    Code:") + 1:])
        self.boot = {}
        if "BootstrapMethods:" in verbose:
            table = verbose.split("BootstrapMethods:")[1]
            for entry in re.split(r"\n(?=  \d+: )", table):
                head = re.match(r"\s*(\d+): #\d+ REF_invokeStatic ([^\n]+)", entry)
                if not head:
                    continue
                handle = re.search(r"REF_(invokeStatic|newInvokeSpecial|invokeVirtual|invokeSpecial|invokeInterface) "
                                   r"([\w/$]+)\.([\w$<>\"]+):(\([^\s]*)", entry.split("Method arguments:")[-1])
                self.boot[int(head[1])] = (head[2], handle and handle.groups())

    def method(self, name, descriptor):
        params = descriptor[:descriptor.index(")") + 1]
        return self.methods[(name, params)]


def parse_code(lines):
    """Instructions by offset: (opcode, operand text, comment, branch targets)."""
    out, i = {}, 0
    order = []
    while i < len(lines):
        line = lines[i]
        match = re.match(r"\s*(\d+): (\w+)\s*(.*)", line)
        if not match:
            i += 1
            continue
        offset, op, rest = int(match[1]), match[2], match[3]
        comment = rest.split("//", 1)[1].strip() if "//" in rest else ""
        operand = rest.split("//", 1)[0].strip()
        targets = []
        if op in ("tableswitch", "lookupswitch"):
            i += 1
            while "}" not in lines[i]:
                targets.append(int(lines[i].split(":")[1]))
                i += 1
        out[offset] = (op, operand, comment, targets)
        order.append(offset)
        i += 1
    following = {a: b for a, b in zip(order, order[1:])}
    return out, following


BRANCHES = {"ifeq", "ifne", "iflt", "ifge", "ifgt", "ifle", "if_icmpeq", "if_icmpne", "if_icmplt", "if_icmpge",
            "if_icmpgt", "if_icmple", "if_acmpeq", "if_acmpne", "ifnull", "ifnonnull"}
BRANCH_POPS = {op: (2 if op.startswith("if_") else 1) for op in BRANCHES}


class Interpreter:
    def __init__(self, jar):
        self.classes = {}
        self.jar = jar
        self.statics = {}
        self.registered = {}
        self.colors = self._enum_names(DYE_COLOR)
        self.states = self._enum_names(WEATHER_STATE)
        self.copper_prefixes = self._copper_prefixes()
        self.fork_depth = 0

    def cls(self, name):
        if name not in self.classes:
            self.classes[name] = ClassCode(self.jar, name)
        return self.classes[name]

    def _enum_names(self, name):
        code, following = self.cls(name).method("<clinit>", "()V")
        names, offset = [], 0
        while offset in code:
            op, _, comment, _ = code[offset]
            if op == "putstatic" and re.search(r"Field [A-Z_]+:L" + re.escape(name) + ";", comment):
                names.append(re.search(r"Field ([A-Z_]+):", comment)[1])
            offset = following.get(offset, -1)
        if not names:
            raise ValueError(f"no constants in {name}")
        return names

    def _copper_prefixes(self):
        code, following = self.cls(COPPER_COLLECTION).method("<clinit>", "()V")
        strings = [c.split("String", 1)[1].strip() if c.split("String", 1)[1].strip() else ""
                   for (op, _, c, _) in code.values() if op.startswith("ldc") and c.startswith("String")]
        expected = ["", "exposed_", "weathered_", "oxidized_", "waxed_", "waxed_exposed_", "waxed_weathered_",
                    "waxed_oxidized_"]
        if strings[:8] != expected or self.states != ["UNAFFECTED", "EXPOSED", "WEATHERED", "OXIDIZED"]:
            raise ValueError(f"copper collection prefixes changed: {strings}")
        return {(waxed, state): expected[4 * waxed + i] for waxed in (0, 1) for i, state in enumerate(self.states)}

    # Values of static fields of the three registration classes.
    def static(self, owner, field):
        if owner in REGISTRATION_CLASSES:
            if owner not in self.statics:
                self.statics[owner] = {}
                self.run(owner, "<clinit>", "()V", [])
            if field not in self.statics[owner]:
                raise ValueError(f"{owner}.{field} read before initialization")
            return self.statics[owner][field]
        if owner == DYE_COLOR:
            return Enum("color", field)
        if owner == WEATHER_STATE:
            return Enum("state", field)
        if owner == COLOR_COLLECTION and field == "VALUES":
            return Coll("color", {c: Enum("color", c) for c in self.colors})
        return Opaque(f"{owner}.{field}")

    def register(self, key, props):
        if not isinstance(key, Key) or not isinstance(props, Props):
            raise ValueError(f"unknown registration form: {key!r} {props!r}")
        if self.fork_depth:
            raise ValueError(f"{key.name} registers inside a branch")
        if key.name in self.registered:
            raise ValueError(f"{key.name} registered twice")
        self.registered[key.name] = props.copy()
        return Key(key.name)

    def call_function(self, fn, args):
        """Invoke a functional value (`Function.apply` and friends)."""
        if isinstance(fn, Lambda):
            if fn.kind == "invokeStatic":
                return self.invoke("invokestatic", fn.owner, fn.method, fn.descriptor, [*fn.captured, *args])
            return Opaque(f"{fn.owner}.{fn.method}")
        return Opaque("function result")

    def name_of(self, value):
        if isinstance(value, Key):
            return value.name
        if isinstance(value, str):
            return value
        raise ValueError(f"not a registry name: {value!r}")

    def invoke(self, op, owner, method, descriptor, args):
        """Model a call; `args` include the receiver for instance calls."""
        returns = descriptor_returns(descriptor)
        if owner == PROPERTIES:
            if method == "of":
                return Props()
            if method in ("ofFullCopy", "ofLegacyCopy"):
                source = args[0]
                if not isinstance(source, Key):
                    raise ValueError(f"copy of unknown block {source!r}")
                return self.registered[source.name].copy()
            receiver = args[0]
            if not isinstance(receiver, Props):
                raise ValueError(f"Properties.{method} on {receiver!r}")
            if method == "strength":
                receiver.destroy = args[1]
                receiver.resistance = max(0.0, args[2] if len(args) > 2 else args[1])
            elif method == "destroyTime":
                receiver.destroy = args[1]
            elif method == "explosionResistance":
                receiver.resistance = max(0.0, args[1])
            elif method == "instabreak":
                receiver.destroy, receiver.resistance = 0.0, 0.0
            elif method == "requiresCorrectToolForDrops":
                receiver.requires = True
            elif returns != "L" + PROPERTIES + ";":
                return Opaque(f"Properties.{method}")
            return receiver
        if owner == BLOCKS and method == "register" and descriptor.startswith("(Lnet/minecraft/resources/ResourceKey;"
                                                                                 "Ljava/util/function/Function;"):
            return self.register(args[0], args[2])
        if owner in REGISTRATION_CLASSES:
            return self.run(owner, method, descriptor, args)
        if owner == "net/minecraft/references/BlockItemId":
            if method == "create":
                return Key(*args)
            if method == "block":
                return Key(self.name_of(args[0]))
        if owner == "net/minecraft/resources/Identifier" and method == "withDefaultNamespace":
            return args[0]
        if owner == "net/minecraft/resources/ResourceKey" and method == "create":
            # Keys built from other registries' keys (pottery sherds, discs)
            # stay unnamed; nothing here reads them.
            return Key(args[1] if isinstance(args[1], str) else None)
        if owner == COLOR_COLLECTION:
            return self.color_collection(method, args)
        if owner in (COPPER_COLLECTION, BY_STATE):
            return self.copper_collection(owner, method, args)
        if owner in FUNCTIONAL and method == "apply":
            return self.call_function(args[0], args[1:])
        return None if returns == "V" else Opaque(f"{owner}.{method}")

    def color_collection(self, method, args):
        if method == "create":
            return Coll("color", {c: args[0] for c in self.colors})
        if method == "prefixWithColor":
            return args[0].map(lambda c, v: f"{c.lower()}_{v}")
        if method == "map":
            return args[0].map(lambda c, v: self.call_function(args[1], [v]))
        if method == "pick":
            return args[0].items[args[1].name]
        if method == "registerBlocks":
            ids, registrar, _factory, props = args
            return ids.map(lambda c, key: self.call_function(
                registrar, [key, Opaque("factory"), self.call_function(props, [Enum("color", c)])]))
        if method == "zipMap":
            first, second, fn = args
            if first.items.keys() != second.items.keys():
                raise ValueError("colour collections differ")
            return Coll("color", {c: self.call_function(fn, [v, second.items[c]]) for c, v in first.items.items()})
        raise ValueError(f"unknown ColorCollection.{method}")

    def copper_collection(self, owner, method, args):
        if owner == BY_STATE:
            if method == "<init>":
                return Coll("state", dict(zip(self.states, args[1:])))
            if method == "pick":
                return args[0].items[args[1].name]
            raise ValueError(f"unknown ByState.{method}")
        if method == "create":
            return Coll("copper", {(w, s): args[0] for w in (0, 1) for s in self.states})
        if method == "same":
            return Coll("copper", {(w, s): v for w in (0, 1) for s, v in args[0].items.items()})
        if method == "prefixWithState":
            return args[0].map(lambda k, v: self.copper_prefixes[k] + v)
        if method == "map":
            return args[0].map(lambda k, v: self.call_function(args[1], [v]))
        if method in ("weathering", "waxed"):
            waxed = int(method == "waxed")
            return Coll("state", {s: v for (w, s), v in args[0].items.items() if w == waxed})
        if method == "registerBlocks":
            ids, registrar, _waxed, _weathering, props = args
            return ids.map(lambda k, key: self.call_function(
                registrar, [key, Opaque("factory"), self.call_function(props, [Enum("state", k[1])])]))
        raise ValueError(f"unknown WeatheringCopperCollection.{method}")

    def run(self, owner, method, descriptor, args):
        code, following = self.cls(owner).method(method, descriptor)
        static = method == "<clinit>" or True
        locals_ = {}
        index = 0
        for value, kind in zip(args, ["L"] * len(args)):
            locals_[index] = value
            index += 1
        return self.execute(owner, code, following, 0, [], locals_, 0)

    def execute(self, owner, code, following, pc, stack, locals_, steps):
        cls = self.cls(owner)
        while True:
            steps += 1
            if steps > 20000:
                raise ValueError("bytecode evaluation did not finish")
            op, operand, comment, targets = code[pc]
            nxt = following.get(pc)
            if op in ("areturn", "freturn", "ireturn"):
                return stack.pop()
            if op == "return":
                return None
            if op == "athrow":
                # An unreachable switch default (`MatchException`): not an outcome.
                return DEAD
            if op in BRANCHES or op in ("tableswitch", "lookupswitch"):
                for _ in range(BRANCH_POPS.get(op, 1)):
                    stack.pop()
                outcomes = []
                destinations = targets if targets else [int(operand), nxt]
                self.fork_depth += 1
                try:
                    for destination in dict.fromkeys(destinations):
                        frame = copy.deepcopy((stack, locals_))
                        outcomes.append(self.execute(owner, code, following, destination, frame[0], frame[1], steps))
                finally:
                    self.fork_depth -= 1
                outcomes = [o for o in outcomes if o is not DEAD] or [DEAD]
                if any(o != outcomes[0] for o in outcomes):
                    raise ValueError(f"branch outcomes differ in {owner}: {outcomes}")
                return outcomes[0]
            if op == "goto":
                pc = int(operand)
                continue
            m = re.match(r"([afi])load(?:_(\d))?$", op)
            if m:
                stack.append(locals_.get(int(m[2] if m[2] is not None else operand)))
            elif re.match(r"[afi]store(?:_\d)?$", op):
                locals_[int(op.split("_")[1] if "_" in op else operand)] = stack.pop()
            elif op in ("ldc", "ldc_w"):
                kind, _, text = comment.partition(" ")
                if kind == "float":
                    stack.append(float(text.rstrip("f")))
                elif kind == "int":
                    stack.append(int(text))
                elif kind == "String":
                    stack.append(text)
                else:
                    stack.append(Opaque(comment))
            elif op.startswith("fconst_"):
                stack.append(float(op[-1]))
            elif op.startswith("iconst_"):
                stack.append(-1 if op.endswith("m1") else int(op[-1]))
            elif op in ("bipush", "sipush"):
                stack.append(int(operand))
            elif op == "aconst_null":
                stack.append(None)
            elif op == "dup":
                stack.append(stack[-1])
            elif op == "pop":
                stack.pop()
            elif op == "checkcast":
                pass
            elif op in ("iaload", "aaload"):
                stack.pop()
                stack.pop()
                stack.append(Opaque("array element"))
            elif op == "new":
                stack.append(Opaque(comment))
            elif op == "getstatic":
                target, field = re.search(r"Field (?:([\w/$]+)\.)?([\w$]+):", comment).groups()
                stack.append(self.static(target or owner, field))
            elif op == "putstatic":
                target, field = re.search(r"Field (?:([\w/$]+)\.)?([\w$]+):", comment).groups()
                self.statics.setdefault(target or owner, {})[field] = stack.pop()
            elif op == "getfield":
                stack.pop()
                stack.append(Opaque(comment))
            elif op == "invokedynamic":
                boot, desc = re.search(r"InvokeDynamic #(\d+):\w+:(\([^\s]*)", comment).groups()
                captured = [stack.pop() for _ in descriptor_args(desc)][::-1]
                factory, handle = cls.boot[int(boot)]
                if "LambdaMetafactory" in factory and handle:
                    kind, target, method, target_desc = handle
                    stack.append(Lambda(target, method, target_desc, captured, kind))
                else:
                    stack.append(Opaque(f"indy {factory}"))
            elif op in ("invokestatic", "invokevirtual", "invokeinterface", "invokespecial"):
                target, method, desc = re.search(r"(?:Interface)?Method (?:([\w/$]+)\.)?([\w$<>\"]+):(\([^\s]*)",
                                                 comment).groups()
                method = method.strip('"')
                target = target or owner
                count = len(descriptor_args(desc)) + (0 if op == "invokestatic" else 1)
                args = [stack.pop() for _ in range(count)][::-1]
                if op == "invokespecial" and method == "<init>":
                    result = self.invoke(op, target, method, desc, args)
                    # The constructed object stays on the stack from `new`/`dup`.
                    if isinstance(result, Coll):
                        stack[-1] = result
                else:
                    result = self.invoke(op, target, method, desc, args)
                    if descriptor_returns(desc) != "V":
                        stack.append(result)
            else:
                raise ValueError(f"unsupported opcode {op} in {owner}")
            pc = nxt


def block_properties(jar):
    """Every block ID registered by `Blocks`, with its mining properties."""
    interpreter = Interpreter(jar)
    interpreter.static(BLOCKS, "AIR")
    return {name: props.key() for name, props in interpreter.registered.items()}


ITEMS = "net/minecraft/world/item/Items"


def item_registry_ids(jar):
    """Registry IDs of each `Items` field, read from the `BlockItemIds` or
    `ItemIds` constant its registration passes (several for colour and copper
    collections, in collection order). Field names can differ from IDs
    (`Items.DRY_SHORT_GRASS` is `short_dry_grass`)."""
    interpreter = Interpreter(jar)
    code = subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", ITEMS.replace("/", ".")], text=True)
    clinit = code.split("  static {};")[1]
    result, start = {}, 0
    for match in re.finditer(r"putstatic\s+#[^\n]*// Field ([A-Z0-9_]+):L([\w/$]+);", clinit):
        chunk = clinit[start:match.start()]
        start = match.end()
        if match[2] not in ("net/minecraft/world/item/Item", COLOR_COLLECTION, COPPER_COLLECTION):
            continue
        ids = dict.fromkeys(re.findall(r"getstatic\s+#\d+\s+// Field (net/minecraft/references/(?:Block)?ItemIds)\.(\w+):",
                                       chunk))
        if len(ids) != 1:
            raise ValueError(f"Items.{match[1]} registers {len(ids)} IDs")
        value = interpreter.static(*next(iter(ids)))
        keys = list(value.items.values()) if isinstance(value, Coll) else [value]
        # Keys derived from another registry (spawn eggs, sherds, discs) stay
        # `None`; a caller that needs one of them fails on it.
        named = all(isinstance(k, Key) and k.name for k in keys)
        result[match[1]] = [k.item for k in keys] if named else None
    return result
