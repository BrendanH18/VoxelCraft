"""Inspect Items registrations, following nested property callbacks.

Used by fixture extractors only. The resulting tables never load at runtime.
"""
import re
import subprocess

COLORS = ("white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray",
          "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black")
COLOR_NAMES = {"DYED_TERRACOTTA": "terracotta", "DYED_SHULKER_BOX": "shulker_box",
               "DYED_BUNDLE": "bundle", "DYED_CANDLE": "candle"}


def provider_associations(jar_path, prefix):
    def inspect(*flags):
        return subprocess.check_output(["javap", "-classpath", str(jar_path), *flags,
                                        "net.minecraft.world.item.Items"], text=True)
    code = inspect("-c", "-p")
    verbose = inspect("-v", "-p").split("BootstrapMethods:")[1]
    methods = {re.search(r"(lambda\$static\$\d+)\(", block).group(1): block
               for block in code.split("\n\n")
               if re.search(r"(lambda\$static\$\d+)\(", block.split("\n")[0])}
    boot = {int(block.split(":")[0].strip()): re.search(r"Items\.(lambda\$static\$\d+)", block).group(1)
            for block in re.split(r"\n(?=  \d+:)", verbose)
            if re.search(r"Items\.(lambda\$static\$\d+)", block)}

    def providers(block, visiting=()):
        fields = re.findall(r"ContextIntProviders\." + prefix + r"([A-Z_]+):", block)
        for number in re.findall(r"InvokeDynamic #(\d+):", block):
            callback = boot.get(int(number))
            if callback and callback not in visiting:
                fields += providers(methods[callback], (*visiting, callback))
        return set(fields)

    clinit = code.split("  static {};")[1]
    start, result = 0, {}
    for match in re.finditer(r"putstatic\s+#[^\n]*// Field ([A-Z0-9_]+):([^\n]+)", clinit):
        block = clinit[start:match.start()]
        start = match.end()
        fields = providers(block)
        if len(fields) > 1:
            raise ValueError(f"ambiguous {prefix} registration for {match[1]}: {fields}")
        if fields:
            name = match[1].lower()
            names = [name]
            if "ColorCollection" in match[2]:
                suffix = COLOR_NAMES.get(match[1], name)
                names = [f"{color}_{suffix}" for color in COLORS]
            for name in names:
                result[name] = next(iter(fields)).lower()
    return result
