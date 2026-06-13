(goal
  (fp.lt (_ +zero 8 24) x0)
  (fp.lt (_ +zero 8 24) x1)
  (fp.lt (_ +zero 8 24) x2)
  (fp.lt (_ +zero 8 24) x3)
  (not (fp.isNaN x0))
  (not (fp.isNaN x1))
  (not (fp.isNaN x2))
  (not (fp.isNaN x3))
  (fp.lt (_ +zero 8 24)
         (fp.add roundNearestTiesToEven
                 (fp.add roundNearestTiesToEven
                         (fp.add roundNearestTiesToEven x0 x1)
                         x2)
                 x3)))