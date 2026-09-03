{
  "proj": {
    "time_scale": "seconds",
    "tech_milestones": [
      [
        "automation-science-pack",
        true
      ],
      [
        "logistic-science-pack",
        true
      ],
      [
        "chemical-science-pack",
        true
      ],
      [
        "military-science-pack",
        true
      ],
      [
        "quality-module",
        true
      ],
      [
        "production-science-pack",
        true
      ],
      [
        "rocket-silo",
        true
      ],
      [
        "utility-science-pack",
        true
      ],
      [
        "space-science-pack",
        true
      ],
      [
        "planet-discovery-fulgora",
        true
      ],
      [
        "planet-discovery-gleba",
        true
      ],
      [
        "planet-discovery-vulcanus",
        true
      ],
      [
        "agricultural-science-pack",
        true
      ],
      [
        "electromagnetic-science-pack",
        true
      ],
      [
        "metallurgic-science-pack",
        true
      ],
      [
        "epic-quality",
        true
      ],
      [
        "planet-discovery-aquilo",
        true
      ],
      [
        "cryogenic-science-pack",
        true
      ],
      [
        "legendary-quality",
        true
      ],
      [
        "promethium-science-pack",
        true
      ]
    ],
    "recipe_productivity": {
      "advanced-carbonic-asteroid-crushing": 0.1,
      "advanced-metallic-asteroid-crushing": 0.1,
      "advanced-oxide-asteroid-crushing": 0.1,
      "ammonia-rocket-fuel": 0.1,
      "bioplastic": 0.1,
      "carbonic-asteroid-crushing": 0.1,
      "casting-low-density-structure": 0.1,
      "casting-steel": 0.1,
      "low-density-structure": 0.1,
      "metallic-asteroid-crushing": 0.1,
      "oxide-asteroid-crushing": 0.1,
      "plastic-bar": 0.1,
      "processing-unit": 0.1,
      "rocket-fuel": 0.1,
      "rocket-fuel-from-jelly": 0.1,
      "rocket-part": 0.1,
      "scrap-recycling": 0.1,
      "steel-plate": 0.1
    },
    "ignore_productivity": false,
    "mining_productivity": 0.3,
    "all_accessible": false
  },
  "name": "2.1测试",
  "factories": [
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+]",
      "target": [
        [
          {
            "Item": [
              "electromagnetic-science-pack",
              4
            ]
          },
          1.0
        ]
      ],
      "target_group": [],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 2,
          "enumerate_modules": [
            [
              "speed-module-3",
              4
            ],
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": false,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+][+]",
      "target": [],
      "target_group": [
        {
          "constant": 1.0,
          "coefficients": [
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  0
                ]
              },
              1.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  1
                ]
              },
              2.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  2
                ]
              },
              3.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  3
                ]
              },
              4.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  4
                ]
              },
              6.0
            ]
          ]
        }
      ],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 3,
          "enumerate_modules": [
            [
              "speed-module-3",
              4
            ],
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ]
          ],
          "enumerate_beacons": []
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": false,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+]",
      "target": [],
      "target_group": [
        {
          "constant": 1.0,
          "coefficients": [
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  1
                ]
              },
              2.0
            ]
          ]
        }
      ],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 2,
          "enumerate_modules": [
            [
              "speed-module-3",
              4
            ],
            [
              "efficiency-module-3",
              4
            ],
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 3,
          "enumerate_modules": [],
          "enumerate_beacons": []
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": true,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+][+]",
      "target": [
        [
          {
            "Item": [
              "electromagnetic-science-pack",
              4
            ]
          },
          1.0
        ]
      ],
      "target_group": [],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 2,
          "enumerate_modules": [
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": false,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "vulcanus",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+][+]",
      "target": [
        [
          {
            "Item": [
              "iron-plate",
              4
            ]
          },
          1.0
        ]
      ],
      "target_group": [],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [
            {
              "recipe": [
                "molten-copper-from-lava",
                2
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "hazard-concrete-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "iron-plate",
                3
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "molten-copper-from-lava",
                3
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "iron-plate",
                4
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-furnace-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "molten-copper-from-lava",
                4
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                4
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "concrete-from-molten-iron",
                2
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                2
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-furnace-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                3
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "concrete-from-molten-iron",
                3
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "concrete-from-molten-iron",
                4
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "hazard-concrete-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-brick",
                2
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "steam-condensation",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe",
                2
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-furnace",
                1
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "hazard-concrete",
                2
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "calcite-recycling",
                0
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe",
                3
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "calcite-recycling",
                1
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "hazard-concrete",
                3
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-brick",
                3
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "concrete-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-furnace-recycling",
                1
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-brick",
                4
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "stone-furnace-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "molten-iron-from-lava",
                1
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "iron-plate",
                2
              ],
              "machine": [
                "electric-furnace",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "hazard-concrete-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "acid-neutralisation",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            }
          ],
          "machine_preferences": [],
          "alternative_count": 2,
          "enumerate_modules": [
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [
            {
              "resource": "sulfuric-acid-geyser",
              "machine": [
                "pumpjack",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "resource": "calcite",
              "machine": [
                "big-mining-drill",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              },
              "fuel": null
            }
          ],
          "alternative_count": 1,
          "enumerate_modules": [],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": [
            {
              "generator": [
                "steam-turbine",
                4
              ],
              "fluid": "steam",
              "temperature": 500
            }
          ]
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [
        [
          1,
          1
        ],
        [
          3,
          0
        ],
        [
          1,
          0
        ],
        [
          0,
          9
        ],
        [
          0,
          34
        ],
        [
          0,
          13
        ],
        [
          0,
          1
        ],
        [
          0,
          22
        ],
        [
          0,
          0
        ],
        [
          0,
          15
        ],
        [
          0,
          27
        ],
        [
          0,
          18
        ],
        [
          0,
          17
        ],
        [
          0,
          4
        ],
        [
          0,
          25
        ],
        [
          0,
          35
        ],
        [
          0,
          32
        ],
        [
          0,
          26
        ],
        [
          0,
          3
        ],
        [
          0,
          16
        ],
        [
          0,
          21
        ],
        [
          0,
          33
        ],
        [
          0,
          31
        ],
        [
          0,
          2
        ],
        [
          0,
          7
        ],
        [
          0,
          23
        ],
        [
          0,
          29
        ],
        [
          0,
          12
        ],
        [
          0,
          19
        ],
        [
          0,
          6
        ],
        [
          0,
          10
        ],
        [
          0,
          24
        ],
        [
          0,
          14
        ],
        [
          0,
          20
        ],
        [
          0,
          8
        ],
        [
          0,
          28
        ],
        [
          0,
          11
        ],
        [
          0,
          5
        ],
        [
          0,
          30
        ]
      ],
      "strict_source": true,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+][+][+]",
      "target": [],
      "target_group": [
        {
          "constant": 16.0,
          "coefficients": [
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  0
                ]
              },
              1.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  1
                ]
              },
              2.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  2
                ]
              },
              3.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  3
                ]
              },
              4.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  4
                ]
              },
              6.0
            ]
          ]
        }
      ],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ],
            [
              "productivity-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ]
          ],
          "enumerate_beacons": []
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": false,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "vulcanus",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "新工厂[+][+][+]",
      "target": [
        [
          {
            "Item": [
              "iron-plate",
              4
            ]
          },
          1.0
        ]
      ],
      "target_group": [],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [
            {
              "recipe": [
                "casting-low-density-structure",
                2
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe",
                0
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "grenade-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe",
                0
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "molten-copper-from-lava",
                0
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "low-density-structure-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "heavy-oil-cracking",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "plastic-bar",
                3
              ],
              "machine": [
                "cryogenic-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "coal-recycling",
                0
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe",
                1
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                0
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                1
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                4
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                2
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe",
                2
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-pipe-to-ground",
                3
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "acid-neutralisation",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                0
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "light-oil-cracking",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "electronic-circuit-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "molten-iron-from-lava",
                0
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "coal-liquefaction",
                0
              ],
              "machine": [
                "oil-refinery",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "copper-cable-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "plastic-bar",
                2
              ],
              "machine": [
                "cryogenic-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                1
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "copper-cable",
                3
              ],
              "machine": [
                "electromagnetic-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "steam-condensation",
                0
              ],
              "machine": [
                "chemical-plant",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "grenade-recycling",
                1
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "casting-low-density-structure",
                3
              ],
              "machine": [
                "foundry",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "low-density-structure-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "coal-liquefaction",
                4
              ],
              "machine": [
                "oil-refinery",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "grenade-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "copper-cable",
                2
              ],
              "machine": [
                "electromagnetic-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                2
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": []
              },
              "fuel": null
            },
            {
              "recipe": [
                "electronic-circuit",
                3
              ],
              "machine": [
                "electromagnetic-plant",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "grenade",
                1
              ],
              "machine": [
                "assembling-machine-3",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "pipe-to-ground-recycling",
                4
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "recipe": [
                "grenade-recycling",
                3
              ],
              "machine": [
                "recycler",
                4
              ],
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              },
              "fuel": null
            }
          ],
          "machine_preferences": [],
          "alternative_count": 2,
          "enumerate_modules": [
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [
            {
              "resource": "coal",
              "machine": [
                "big-mining-drill",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ],
                  [
                    "quality-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "resource": "calcite",
              "machine": [
                "big-mining-drill",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "speed-module-3",
                    4
                  ],
                  [
                    "speed-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              },
              "fuel": null
            },
            {
              "resource": "sulfuric-acid-geyser",
              "machine": [
                "pumpjack",
                4
              ],
              "module_config": {
                "modules": [
                  [
                    "productivity-module-3",
                    4
                  ],
                  [
                    "speed-module-3",
                    4
                  ]
                ],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              },
              "fuel": null
            }
          ],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "productivity-module-3",
              4
            ],
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        2
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 1.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": [
            {
              "generator": [
                "steam-turbine",
                4
              ],
              "fluid": "steam",
              "temperature": 500
            }
          ]
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [
        [
          3,
          0
        ],
        [
          0,
          21
        ],
        [
          0,
          37
        ],
        [
          0,
          13
        ],
        [
          0,
          20
        ],
        [
          0,
          35
        ],
        [
          1,
          1
        ],
        [
          0,
          3
        ],
        [
          0,
          11
        ],
        [
          0,
          16
        ],
        [
          1,
          2
        ],
        [
          0,
          14
        ],
        [
          1,
          0
        ],
        [
          0,
          34
        ],
        [
          0,
          10
        ],
        [
          0,
          17
        ],
        [
          0,
          12
        ],
        [
          0,
          15
        ],
        [
          0,
          26
        ],
        [
          0,
          25
        ],
        [
          0,
          29
        ],
        [
          0,
          9
        ],
        [
          0,
          4
        ],
        [
          0,
          1
        ],
        [
          0,
          5
        ],
        [
          0,
          18
        ],
        [
          0,
          0
        ],
        [
          0,
          19
        ],
        [
          0,
          27
        ],
        [
          0,
          24
        ],
        [
          0,
          6
        ],
        [
          0,
          33
        ],
        [
          0,
          36
        ],
        [
          0,
          7
        ],
        [
          0,
          22
        ],
        [
          0,
          30
        ],
        [
          0,
          8
        ],
        [
          0,
          23
        ],
        [
          0,
          28
        ],
        [
          0,
          32
        ],
        [
          0,
          38
        ],
        [
          0,
          2
        ],
        [
          0,
          31
        ]
      ],
      "strict_source": true,
      "strict_sink": false
    },
    {
      "factory": {
        "planet": "fulgora",
        "surface": null,
        "major_quality": 4,
        "debug": false
      },
      "name": "粉瓶",
      "target": [],
      "target_group": [
        {
          "constant": 16.0,
          "coefficients": [
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  0
                ]
              },
              1.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  1
                ]
              },
              2.0
            ],
            [
              {
                "Item": [
                  "electromagnetic-science-pack",
                  2
                ]
              },
              3.0
            ]
          ]
        }
      ],
      "external": [],
      "mechanics": [
        {
          "type": "factorio:recipe",
          "instances": [],
          "machine_preferences": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ],
            [
              "productivity-module-3",
              4
            ]
          ],
          "enumerate_beacons": [
            {
              "module_config": {
                "modules": [],
                "beacons": [
                  {
                    "modules": [
                      [
                        [
                          "speed-module-3",
                          4
                        ],
                        1
                      ]
                    ],
                    "beacon": [
                      "beacon",
                      4
                    ],
                    "count": 1,
                    "share": 8.0
                  }
                ]
              }
            }
          ]
        },
        {
          "type": "factorio:mining",
          "instances": [],
          "alternative_count": 1,
          "enumerate_modules": [
            [
              "quality-module-3",
              4
            ],
            [
              "speed-module-3",
              4
            ]
          ],
          "enumerate_beacons": []
        },
        {
          "type": "factorio:item-fuel",
          "instances": []
        },
        {
          "type": "factorio:generator",
          "instances": []
        },
        {
          "type": "factorio:boiler",
          "instances": []
        },
        {
          "type": "factorio:reactor",
          "instances": []
        },
        {
          "type": "factorio:plant",
          "instances": []
        },
        {
          "type": "factorio:spoil",
          "instances": []
        },
        {
          "type": "factorio:fluid-fuel",
          "instances": []
        },
        {
          "type": "factorio:fluid-heat",
          "instances": []
        },
        {
          "type": "factorio:item-launch",
          "instances": []
        }
      ],
      "instances": [],
      "strict_source": false,
      "strict_sink": false
    }
  ]
}