package com.fatecomputers.fcw.production.machine;

/**
 * A way to power a machine besides FE pushed into its power port: an integration with another mod's
 * energy network (compat packages). One instance per controller, created on its first server tick.
 */
public interface MachinePowerSource {
    /** Every server tick: bring energy if the machine asks for it ({@link MachineControllerBlockEntity#wantedEnergy}). */
    void tick(MachineControllerBlockEntity machine);

    /** The controller is gone or unloaded: leave the network. */
    void remove();
}
