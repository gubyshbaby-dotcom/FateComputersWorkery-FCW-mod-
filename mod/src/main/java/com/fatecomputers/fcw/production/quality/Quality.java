package com.fatecomputers.fcw.production.quality;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.ProductionDifficulty;
import com.mojang.serialization.Codec;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.Locale;

/**
 * Quality carried by items (docs/design/DEFECTS.md, sections 4, 5 and 7). Only a class is stored in an
 * item, so that stacks still merge; an item without the component counts as class A.
 * <ul>
 * <li>{@code fcw:crystal_grade} on boules and wafers: a / b / c;</li>
 * <li>{@code fcw:polish_grade} on wafers: a / b / c;</li>
 * <li>{@code fcw:yield_class} on patterned wafers: s / a / b / c / d.</li>
 * </ul>
 * Defects exist on the difficulty levels engineer and fab ({@link #enabled}); on standard nothing is
 * stamped and every wafer is class A.
 */
public final class Quality {
    private static final DeferredRegister.DataComponents COMPONENTS = DeferredRegister.createDataComponents(Registries.DATA_COMPONENT_TYPE, FcwMod.MODID);

    public static final DeferredHolder<DataComponentType<?>, DataComponentType<String>> CRYSTAL_GRADE = COMPONENTS.registerComponentType(
            "crystal_grade", b -> b.persistent(Codec.STRING).networkSynchronized(ByteBufCodecs.STRING_UTF8));
    public static final DeferredHolder<DataComponentType<?>, DataComponentType<String>> POLISH_GRADE = COMPONENTS.registerComponentType(
            "polish_grade", b -> b.persistent(Codec.STRING).networkSynchronized(ByteBufCodecs.STRING_UTF8));
    public static final DeferredHolder<DataComponentType<?>, DataComponentType<String>> YIELD_CLASS = COMPONENTS.registerComponentType(
            "yield_class", b -> b.persistent(Codec.STRING).networkSynchronized(ByteBufCodecs.STRING_UTF8));

    /** Class of a boule or of the polish of a wafer. */
    public enum Grade {
        A, B, C;

        /** A when both conditions held, B when one did, C when none. */
        public static Grade of(boolean water, boolean noPauses) {
            return water && noPauses ? A : water || noPauses ? B : C;
        }

        public String id() {
            return name().toLowerCase(Locale.ROOT);
        }

        public static Grade parse(String id) {
            return "c".equalsIgnoreCase(id) ? C : "b".equalsIgnoreCase(id) ? B : A;
        }
    }

    /** Yield class of a patterned wafer: by the number of good fields out of 61. */
    public enum YieldClass {
        S(58, 1.25, "95–100 %"), A(52, 1.0, "85–94 %"), B(43, 0.8, "70–84 %"), C(31, 0.6, "50–69 %"), D(0, 0.4, "< 50 %");

        public final int minGood;
        /** Multiplier of the packaging output. */
        public final double multiplier;
        public final String share;

        YieldClass(int minGood, double multiplier, String share) {
            this.minGood = minGood;
            this.multiplier = multiplier;
            this.share = share;
        }

        public static YieldClass ofGood(int good) {
            for (YieldClass c : values()) {
                if (good >= c.minGood) {
                    return c;
                }
            }
            return D;
        }

        public String id() {
            return name().toLowerCase(Locale.ROOT);
        }

        public static YieldClass parse(String id) {
            for (YieldClass c : values()) {
                if (c.id().equalsIgnoreCase(id)) {
                    return c;
                }
            }
            return A;
        }
    }

    private Quality() {
    }

    public static void register(IEventBus modBus) {
        COMPONENTS.register(modBus);
    }

    /** Whether the defect model is on: not on the difficulty "standard". Extension point: "fab" runs the engineer model
     * for now; its extra rules (docs/design/DEFECTS.md, section 10) branch here and in MachineQuality.decide. */
    public static boolean enabled() {
        // fab: the same base model for now; hidden defects, probing, drift and the rest of DEFECTS.md
        // section 10 hook in here when they exist
        return ProductionDifficulty.current() != ProductionDifficulty.STANDARD;
    }

    public static Grade crystal(ItemStack stack) {
        String v = stack.get(CRYSTAL_GRADE.get());
        return v == null ? Grade.A : Grade.parse(v);
    }

    public static Grade polish(ItemStack stack) {
        String v = stack.get(POLISH_GRADE.get());
        return v == null ? Grade.A : Grade.parse(v);
    }

    public static YieldClass yieldClass(ItemStack stack) {
        String v = stack.get(YIELD_CLASS.get());
        return v == null ? YieldClass.A : YieldClass.parse(v);
    }

    /** Class A is the absence of the component: such stacks merge with items made before the model existed. */
    public static void setCrystal(ItemStack stack, Grade grade) {
        if (grade == Grade.A) {
            stack.remove(CRYSTAL_GRADE.get());
        } else {
            stack.set(CRYSTAL_GRADE.get(), grade.id());
        }
    }

    public static void setPolish(ItemStack stack, Grade grade) {
        if (grade == Grade.A) {
            stack.remove(POLISH_GRADE.get());
        } else {
            stack.set(POLISH_GRADE.get(), grade.id());
        }
    }

    public static void setYieldClass(ItemStack stack, YieldClass c) {
        if (c == YieldClass.A) {
            stack.remove(YIELD_CLASS.get());
        } else {
            stack.set(YIELD_CLASS.get(), c.id());
        }
    }
}
