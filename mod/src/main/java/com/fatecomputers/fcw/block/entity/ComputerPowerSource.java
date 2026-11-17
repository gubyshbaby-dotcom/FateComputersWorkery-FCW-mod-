package com.fatecomputers.fcw.block.entity;

/**
 * Питание корпуса компьютера помимо FE, поданного в сам блок: энергосеть другого мода (пакеты
 * совместимости, сейчас AE2). Один экземпляр на компьютер, создаётся на первом тике сервера, когда
 * включено production.requirePower. То же, что MachinePowerSource у машин.
 */
public interface ComputerPowerSource {
    /** Каждый тик сервера: принести энергию, если корпус её просит ({@link ComputerBlockEntity#wantedEnergy}). */
    void tick(ComputerBlockEntity computer);

    /** Компьютер убран или выгружен: выйти из сети. */
    void remove();
}
