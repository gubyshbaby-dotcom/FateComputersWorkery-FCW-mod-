package com.fatecomputers.fcw.cinema;

/**
 * Питание проектора помимо FE, поданного в сам блок: энергосеть другого мода (сейчас AE2). Один
 * экземпляр на проектор, создаётся на первом тике сервера при production.requirePower. То же, что
 * ComputerPowerSource у компьютеров и MachinePowerSource у машин.
 */
public interface ProjectorPowerSource {
    /** Каждый тик сервера: принести энергию, если проектор её просит ({@link ProjectorBlockEntity#wantedEnergy}). */
    void tick(ProjectorBlockEntity projector);

    /** Проектор убран или выгружен: выйти из сети. */
    void remove();
}
