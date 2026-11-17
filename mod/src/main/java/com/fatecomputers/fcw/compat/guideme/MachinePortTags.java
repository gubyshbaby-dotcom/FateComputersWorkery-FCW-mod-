package com.fatecomputers.fcw.compat.guideme;

import com.fatecomputers.fcw.docs.DocStructures;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec;
import com.fatecomputers.fcw.production.machine.Machines;
import guideme.color.ConstantColor;
import guideme.compiler.PageCompiler;
import guideme.compiler.tags.BlockTagCompiler;
import guideme.document.LytErrorSink;
import guideme.document.block.LytBlockContainer;
import guideme.document.block.LytParagraph;
import guideme.document.block.table.LytTable;
import guideme.document.block.table.LytTableRow;
import guideme.libs.mdast.mdx.model.MdxJsxElementFields;
import guideme.scene.GuidebookScene;
import guideme.scene.annotation.InWorldBoxAnnotation;
import guideme.scene.element.SceneElementTagCompiler;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import org.joml.Vector3f;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Set;

/**
 * Ports of a machine in the guidebook, read from the machine itself (manifest and {@link MachineSpec}),
 * so the pages do not repeat cell coordinates:
 *
 * <ul>
 * <li>{@code <MachinePorts machine="arc_furnace" />} inside a {@code <GameScene>} with the machine's
 * structure: a coloured frame on every port, its name in the tooltip;</li>
 * <li>{@code <MachinePortTable machine="arc_furnace" />}: the same ports as a table (port, kind, side).</li>
 * </ul>
 *
 * Port names are the lang keys {@code guide.fcw.port.<machine>.<port>} (assets/fcw_docs/lang).
 */
final class MachinePortTags {
    private static final int INPUT = 0xFF7FCDE0, OUTPUT = 0xFFDDC166, POWER = 0xFFFF5D6C, FLUID = 0xFF5F6CAF, CONTROL = 0xFFEEEEEE;

    private static final List<String> ORDER = List.of("item_in", "item_both", "item_catalyst", "item", "item_out", "energy", "fluid",
            "gui", "switch", "data");

    private MachinePortTags() {
    }

    @Nullable
    private static MachineDef machine(PageCompiler compiler, LytErrorSink errors, MdxJsxElementFields el) {
        String name = el.getAttributeString("machine", "");
        MachineDef def = Machines.byName(name);
        if (def == null) {
            errors.appendError(compiler, "Unknown machine: " + name, el);
        }
        return def;
    }

    @Nullable
    private static MachineSpec.Role role(MachineDef def, MachineDef.Port port) {
        for (MachineSpec.PortRole r : def.spec.portRoles()) {
            if (r.port().equals(port.id())) {
                return r.role();
            }
        }
        return null;
    }

    private static Component name(MachineDef def, MachineDef.Port port) {
        return Component.translatable("guide.fcw.port." + def.name + "." + port.id());
    }

    /** What flows through the port, as a lang key suffix. */
    private static String kind(MachineDef def, MachineDef.Port port) {
        MachineSpec.Role role = role(def, port);
        if (port.kind().equals("item") && role != null) {
            return switch (role) {
                case INPUT -> "item_in";
                case OUTPUT -> "item_out";
                case BOTH -> "item_both";
                case CATALYST -> "item_catalyst";
            };
        }
        return port.kind();
    }

    private static int color(MachineDef def, MachineDef.Port port) {
        return switch (kind(def, port)) {
            case "item_in", "item_both", "item_catalyst", "item" -> INPUT;
            case "item_out" -> OUTPUT;
            case "energy" -> POWER;
            case "fluid" -> FLUID;
            default -> CONTROL;
        };
    }

    static final class Annotations implements SceneElementTagCompiler {
        @Override
        public Set<String> getTagNames() {
            return Set.of("MachinePorts");
        }

        @Override
        public void compile(GuidebookScene scene, PageCompiler compiler, LytErrorSink errors, MdxJsxElementFields el) {
            MachineDef def = machine(compiler, errors, el);
            if (def == null) {
                return;
            }
            for (MachineDef.Port port : def.ports) {
                BlockPos pos = DocStructures.guideCell(def, port.ax(), port.ay(), port.az());
                Direction face = DocStructures.face(port.face());
                // a thin slab on the face the port opens to
                Vector3f min = new Vector3f(pos.getX(), pos.getY(), pos.getZ());
                Vector3f max = new Vector3f(pos.getX() + 1, pos.getY() + 1, pos.getZ() + 1);
                for (int axis = 0; axis < 3; axis++) {
                    int step = face.getNormal().get(Direction.Axis.values()[axis]);
                    if (step > 0) {
                        min.setComponent(axis, max.get(axis) - 0.1f);
                        max.setComponent(axis, max.get(axis) + 0.05f);
                    } else if (step < 0) {
                        max.setComponent(axis, min.get(axis) + 0.1f);
                        min.setComponent(axis, min.get(axis) - 0.05f);
                    }
                }
                InWorldBoxAnnotation box = new InWorldBoxAnnotation(min, max, new ConstantColor(color(def, port)));
                box.setTooltipContent(Component.translatable("guide.fcw.port.tooltip", name(def, port),
                        Component.translatable("guide.fcw.port.kind." + kind(def, port))));
                scene.addAnnotation(box);
            }
        }
    }

    static final class Table extends BlockTagCompiler {
        @Override
        public Set<String> getTagNames() {
            return Set.of("MachinePortTable");
        }

        @Override
        protected void compile(PageCompiler compiler, LytBlockContainer parent, MdxJsxElementFields el) {
            MachineDef def = machine(compiler, parent, el);
            if (def == null) {
                return;
            }
            LytTable table = new LytTable();
            table.setMarginBottom(5);
            LytTableRow head = table.appendRow();
            head.modifyStyle(style -> style.bold(true));
            for (String column : new String[] {"port", "kind", "side"}) {
                head.appendCell().append(LytParagraph.of(Component.translatable("guide.fcw.port.column." + column).getString()));
            }
            // in the order of the frame colours: inputs, outputs, power, fluids, controls
            List<MachineDef.Port> ports = new ArrayList<>(def.ports);
            ports.sort(Comparator.comparingInt(p -> ORDER.indexOf(kind(def, p))));
            for (MachineDef.Port port : ports) {
                LytTableRow row = table.appendRow();
                row.appendCell().append(LytParagraph.of(name(def, port).getString()));
                row.appendCell().append(LytParagraph.of(Component.translatable("guide.fcw.port.kind." + kind(def, port)).getString()));
                row.appendCell().append(LytParagraph.of(Component.translatable("guide.fcw.port.side." + port.face().getName()).getString()));
            }
            parent.append(table);
        }
    }
}
