---
navigation:
  title: "Machine Components"
  parent: production/index.md
  position: 4
  icon: fcw:electric_motor
item_ids:
  - fcw:iron_plate
  - fcw:steel_plate
  - fcw:iron_frame
  - fcw:steel_frame
  - fcw:enclosure_panel
  - fcw:granite_plate_blank
  - fcw:granite_plate
  - fcw:fireclay
  - fcw:firebrick
  - fcw:refractory_lining
  - fcw:graphite
  - fcw:graphite_electrode
  - fcw:graphite_heater
  - fcw:crucible_blank
  - fcw:quartz_crucible
  - fcw:water_cooled_panel
  - fcw:oven_module
  - fcw:copper_bar
  - fcw:copper_pipe
  - fcw:copper_coil
  - fcw:furnace_transformer
  - fcw:electric_motor
  - fcw:pump
  - fcw:hydraulic_cylinder
  - fcw:pulling_head
  - fcw:linear_stage
  - fcw:conveyor_section
  - fcw:vacuum_chamber_section
  - fcw:control_cabinet_1
  - fcw:control_cabinet_2
  - fcw:control_cabinet_3
  - fcw:steel_wire
  - fcw:diamond_wire
  - fcw:diamond_blade
  - fcw:polishing_head
  - fcw:placement_head
  - fcw:bonding_head
  - fcw:polished_mirror_blank
  - fcw:multilayer_mirror
  - fcw:laser_module
  - fcw:precision_stage
  - fcw:turbo_pump
  - fcw:chiller_unit
  - fcw:proto_chip_controller
  - fcw:proto_chip_memory
  - fcw:proto_cpu_pga
---

# Machine Components

Machines are assembled from components on a build site: [Building Machines](construction.md). The bills below are for the `engineer` level; the `standard` level needs a quarter, rounded up. The `bom` command on the pallet shows the bill of a particular site.

## Bills

### Arc Furnace

| Stage | Time | Components |
|---|---|---|
| Foundation and tower | 3 min | Iron Frame 24, Iron Plate 24 |
| Hearth and lining | 4 min | Refractory Lining 36, Water-Cooled Panel 8, Iron Plate 16 |
| Electrodes and mechanics | 3 min | Graphite Electrode 3, Hydraulic Cylinder 4, Electric Motor 4, Pump 2 |
| Transformer and busbars | 2 min | Furnace Transformer 1, Copper Busbar 12 |
| Casing and terminal | 2 min | Enclosure Panel 32, Tinted Glass 8, Control Cabinet I 1 |

Stages: 5, build time: 14 min.

### Crystal Puller

| Stage | Time | Components |
|---|---|---|
| Frame and base | 2 min | Steel Frame 12, Steel Plate 16 |
| Chamber | 3 min | Vacuum Chamber Section 8, Water-Cooled Panel 6, Refractory Lining 8 |
| Hot zone | 2 min | Quartz Crucible 1, Graphite Heater 4, Furnace Transformer 1, Copper Busbar 6 |
| Pulling and vacuum | 2 min | Pulling Head 1, Pump 2, Electric Motor 2 |
| Casing and terminal | 1.5 min | Enclosure Panel 20, Tinted Glass 6, Control Cabinet I 1 |

Stages: 5, build time: 10.5 min.

### Wafering Cell

| Stage | Time | Components |
|---|---|---|
| Frame and plate | 2 min | Steel Frame 8, Steel Plate 12, Granite Plate 6 |
| Saw | 2 min | Diamond Wire 8, Diamond Blade 2, Linear Stage 3, Electric Motor 4 |
| Polishing and water | 1.5 min | Polishing Head 3, Pump 2, Copper Pipe 8 |
| Casing and terminal | 1.5 min | Enclosure Panel 16, Tinted Glass 8, Control Cabinet II 1 |

Stages: 4, build time: 7 min.

### EUV Scanner Mk2

| Stage | Time | Components |
|---|---|---|
| Frame and casing | 10 min | EUV Scanner Frame 24, EUV Scanner Casing 97, EUV Scanner Vent Casing 12 |
| Vacuum chamber | 10 min | Vacuum Chamber Wall 23, Turbomolecular Pump 2 |
| Drive laser and source | 15 min | CO2 Laser Amplifier 22, Tin Droplet Generator 1, EUV Collector 1, Chiller 9, Power Cabinet 1 |
| Optics | 15 min | Multilayer Mirror 22 |
| Stages and handler | 10 min | Wafer Stage 24, Reticle Stage 9, Wafer Handler Module 13, FOUP Load Port 2 |
| Glazing, ports, commissioning | 10 min | EUV Scanner Glass Casing 29, Coolant Port 2, Data Port 1, Energy Port 1, Exhaust Port 1, Gas Port 2, Item Hatch 2, EUV Scanner Controller 1 |

Stages: 6, build time: 70 min.

### Electronics Line

| Stage | Time | Components |
|---|---|---|
| Frame | 2.5 min | Steel Frame 14, Steel Plate 20 |
| Conveyor | 2.5 min | Conveyor Section 14, Electric Motor 6, Hopper 4 |
| Packaging | 3 min | Bonding Head 2, Linear Stage 2, Pump 1 |
| Board assembly | 3 min | Placement Head 2, Oven Module 2, Linear Stage 2 |
| Casing, terminals, set-up | 2.5 min | Enclosure Panel 30, Tinted Glass 12, Control Cabinet II 2, Silicon Wafer 8 |

Stages: 5, build time: 13.5 min.

The building blocks of the scanner and their recipes: [Scanner blocks](euv-blocks.md).

## Component recipes

<RecipeFor id="fcw:iron_plate" />
<RecipeFor id="fcw:steel_plate" />
<RecipeFor id="fcw:iron_frame" />
<RecipeFor id="fcw:steel_frame" />
<RecipeFor id="fcw:enclosure_panel" />
<RecipeFor id="fcw:granite_plate_blank" />
<RecipeFor id="fcw:granite_plate" />
<RecipeFor id="fcw:fireclay" />
<RecipeFor id="fcw:firebrick" />
<RecipeFor id="fcw:refractory_lining" />
<RecipeFor id="fcw:graphite" />
<RecipeFor id="fcw:graphite_electrode" />
<RecipeFor id="fcw:graphite_heater" />
<RecipeFor id="fcw:crucible_blank" />
<RecipeFor id="fcw:quartz_crucible" />
<RecipeFor id="fcw:water_cooled_panel" />
<RecipeFor id="fcw:oven_module" />
<RecipeFor id="fcw:copper_bar" />
<RecipeFor id="fcw:copper_pipe" />
<RecipeFor id="fcw:copper_coil" />
<RecipeFor id="fcw:furnace_transformer" />
<RecipeFor id="fcw:electric_motor" />
<RecipeFor id="fcw:pump" />
<RecipeFor id="fcw:hydraulic_cylinder" />
<RecipeFor id="fcw:pulling_head" />
<RecipeFor id="fcw:linear_stage" />
<RecipeFor id="fcw:conveyor_section" />
<RecipeFor id="fcw:vacuum_chamber_section" />
<RecipeFor id="fcw:control_cabinet_1" />
<RecipeFor id="fcw:control_cabinet_2" />
<RecipeFor id="fcw:control_cabinet_3" />
<RecipeFor id="fcw:steel_wire" />
<RecipeFor id="fcw:diamond_wire" />
<RecipeFor id="fcw:diamond_blade" />
<RecipeFor id="fcw:polishing_head" />
<RecipeFor id="fcw:placement_head" />
<RecipeFor id="fcw:bonding_head" />
<RecipeFor id="fcw:polished_mirror_blank" />
<RecipeFor id="fcw:multilayer_mirror" />
<RecipeFor id="fcw:laser_module" />
<RecipeFor id="fcw:precision_stage" />
<RecipeFor id="fcw:turbo_pump" />
<RecipeFor id="fcw:chiller_unit" />

The prototype chips for the first scanner are described on the page [Building Machines](construction.md).
