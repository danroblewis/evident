(goal
  (fp.gt x0 (_ +zero 8 24))
  (fp.gt x1 (_ +zero 8 24))
  (fp.gt x2 (_ +zero 8 24))
  (fp.gt x3 (_ +zero 8 24))
  (not (fp.isNaN x0))
  (not (fp.isNaN x1))
  (not (fp.isNaN x2))
  (not (fp.isNaN x3))
  (fp.gt (fp.add roundNearestTiesToEven
                 (fp.add roundNearestTiesToEven
                         (fp.add roundNearestTiesToEven x0 x1)
                         x2)
                 x3)
         (_ +zero 8 24)))